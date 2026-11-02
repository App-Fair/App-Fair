//! Remote image cache for list icons and screenshots. Each URL maps to one reactive
//! `Signal<Option<Arc<Vec<u8>>>>` that a `async_image` piece renders. Loads run on a small pool
//! of background workers (not a thread per icon, which would storm the network and starve the UI),
//! reading from the on-disk cache when present. Recycled list rows asking for the same URL get the
//! same signal, so an icon is fetched at most once.
//!
//! Day's `async_image` only decodes and displays bytes — the fetch (and so the mirror fallback) is
//! ours. A metadata-only repo (e.g. appfair.net) serves no images itself and points at F-Droid-style
//! mirrors for the binaries, so a request to the primary host 404s; [`load`] then retries the same
//! path on each of the repo's mirrors and remembers the one that worked so later icons skip the dead
//! primary. The mirror map is refreshed from the catalog after each sync via [`set_mirror_bases`].

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, OnceLock};

use day::async_image::{AsyncImage, async_image};
use day::prelude::*;

/// A cached image's reactive bytes.
type ImageSig = Signal<Option<Arc<Vec<u8>>>>;

/// Per-repo mirror bases (repo address → its ordered base URLs, primary first) for image fallback.
/// Refreshed from the catalog after each sync (#12 applied to icons/screenshots).
static MIRROR_BASES: Mutex<Vec<(String, Vec<String>)>> = Mutex::new(Vec::new());
/// The base URL that last served an image for a repo, floated to the front of the next fetch so
/// once one icon loads from a mirror the rest skip the primary that doesn't host binaries.
static LAST_GOOD: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// Replace the per-repo mirror map used for image fallback (called after a sync updates the repos'
/// mirror lists). `map` is `(repo_address, mirror_bases)`, mirror_bases primary-first.
pub fn set_mirror_bases(map: Vec<(String, Vec<String>)>) {
    *MIRROR_BASES.lock().unwrap() = map;
}

/// A `async_image` whose bytes follow a reactive URL. As a recycling list row rebinds to a new
/// item, `url_of` yields the new icon URL and the image swaps. The URL closure MUST read the item
/// reactively (e.g. `move || slot.field(|a| a.icon_url())`).
pub fn row_icon(url_of: impl Fn() -> Option<String> + 'static) -> AsyncImage {
    let bytes: ImageSig = Signal::new(None);
    // Mirror the current URL's cached image signal into this row-local signal. Re-runs both when
    // the row rebinds (URL changes) and when that URL's bytes finish loading.
    bind(
        move || url_of().and_then(|u| image_signal(&u).get()),
        move |b: &Option<Arc<Vec<u8>>>| bytes.set(b.clone()),
    );
    async_image(bytes)
}

/// How many icons/screenshots download at once.
const WORKERS: usize = 6;

/// The decoded-icon cache — app-wide (docs/state.md): a CACHE over shared catalog art, so two
/// windows showing the same app read one decode.
#[derive(Clone, Default)]
struct Cache(std::rc::Rc<RefCell<HashMap<String, ImageSig>>>);

impl Ambient for Cache {
    fn create() -> Self {
        Cache::default()
    }
}

impl Cache {
    /// The same shape the `thread_local!` accessor had, so every call site below reads the same.
    fn with<R>(f: impl FnOnce(&RefCell<HashMap<String, ImageSig>>) -> R) -> R {
        f(&Cache::app().0)
    }
}

struct Job {
    url: String,
    setter: Setter<Option<Arc<Vec<u8>>>>,
}

struct Queue {
    jobs: Mutex<VecDeque<Job>>,
    ready: Condvar,
}

fn queue() -> &'static Queue {
    static Q: OnceLock<Queue> = OnceLock::new();
    Q.get_or_init(|| {
        let q = Queue {
            jobs: Mutex::new(VecDeque::new()),
            ready: Condvar::new(),
        };
        // Spawn the worker pool once, on first use.
        for _ in 0..WORKERS {
            std::thread::spawn(worker);
        }
        q
    })
}

fn worker() {
    crate::util::lower_priority();
    let q = queue();
    loop {
        let job = {
            let mut jobs = q.jobs.lock().unwrap();
            loop {
                if let Some(job) = jobs.pop_front() {
                    break job;
                }
                jobs = q.ready.wait(jobs).unwrap();
            }
        };
        load(&job.url, job.setter);
    }
}

/// The signal backing `async_image` for `url`, enqueuing a load on first request.
pub fn image_signal(url: &str) -> ImageSig {
    Cache::with(|c| {
        if let Some(sig) = c.borrow().get(url) {
            return *sig;
        }
        // Detached: cached image signals outlive any single list row.
        let sig = Scope::detached().enter(|| Signal::new(None::<Arc<Vec<u8>>>));
        c.borrow_mut().insert(url.to_string(), sig);
        let q = queue();
        q.jobs.lock().unwrap().push_back(Job {
            url: url.to_string(),
            setter: sig.setter(),
        });
        q.ready.notify_one();
        sig
    })
}

fn load(url: &str, setter: Setter<Option<Arc<Vec<u8>>>>) {
    // The on-disk cache is keyed by the primary URL, so a mirror-served image stays cached under the
    // stable key regardless of which host it came from.
    let path = cache_path(url);
    if let Ok(bytes) = std::fs::read(&path)
        && !bytes.is_empty()
    {
        setter.set(Some(Arc::new(bytes)));
        return;
    }
    // Try the primary host and each mirror of the owning repo, in order (last-good first).
    let (addr, cands) = candidates(url);
    for (base, cand) in cands {
        if let Ok(bytes) = crate::net::get_bytes(&cand)
            && !bytes.is_empty()
            && looks_like_image(&bytes)
        {
            if let Some(a) = &addr
                && !base.is_empty()
            {
                remember_good(a, &base);
            }
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&path, &bytes);
            setter.set(Some(Arc::new(bytes)));
            return;
        }
    }
}

/// The URLs to try for `url`, as `(base, full_url)`: the same path on the primary host and each
/// mirror of the repo it belongs to, most-recently-successful base first. Returns the repo address
/// too (for recording the good base). A URL outside every known repo yields just itself.
fn candidates(url: &str) -> (Option<String>, Vec<(String, String)>) {
    let map = MIRROR_BASES.lock().unwrap();
    let Some((addr, mirrors)) = map.iter().find(|(a, _)| url.starts_with(a.as_str())) else {
        return (None, vec![(String::new(), url.to_string())]);
    };
    let path = &url[addr.len()..];
    let mut bases: Vec<String> = mirrors.clone();
    // Always try the primary host (F-Droid repos list themselves as the first mirror, but don't rely
    // on it).
    if !bases.iter().any(|b| b == addr) {
        bases.insert(0, addr.clone());
    }
    if let Some((_, good)) = LAST_GOOD.lock().unwrap().iter().find(|(a, _)| a == addr)
        && let Some(i) = bases.iter().position(|b| b == good)
    {
        let g = bases.remove(i);
        bases.insert(0, g);
    }
    let addr = addr.clone();
    let cands = bases
        .into_iter()
        .map(|b| {
            let full = format!("{b}{path}");
            (b, full)
        })
        .collect();
    (Some(addr), cands)
}

/// Record the base URL that just served an image for `addr`, so the next icon tries it first.
fn remember_good(addr: &str, base: &str) {
    let mut lg = LAST_GOOD.lock().unwrap();
    match lg.iter_mut().find(|(a, _)| a == addr) {
        Some(entry) => entry.1 = base.to_string(),
        None => lg.push((addr.to_string(), base.to_string())),
    }
}

/// Whether `bytes` begins with a known raster-image signature (PNG/JPEG/GIF/WebP) — so a mirror's
/// HTML error page (or any 200-with-wrong-body) is treated as a miss and never cached as an icon.
fn looks_like_image(b: &[u8]) -> bool {
    b.starts_with(b"\x89PNG\r\n\x1a\n")
        || b.starts_with(&[0xFF, 0xD8, 0xFF])
        || b.starts_with(b"GIF8")
        || (b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP")
}

/// Write `bytes` into the on-disk cache under `url`, so a later request for that URL loads them
/// straight from disk without touching the network. Used to pre-warm the mock catalog's icons.
pub fn preseed(url: &str, bytes: &[u8]) {
    let path = cache_path(url);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, bytes);
}

fn cache_path(url: &str) -> PathBuf {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut h);
    crate::platform::cache_dir()
        .join("imgcache")
        .join(format!("{:016x}", h.finish()))
}
