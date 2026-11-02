// App Fair's Android install bridge. Streams a downloaded APK into a PackageInstaller session and
// commits it; Android shows its own confirm screen, and the session's result comes back through a
// BroadcastReceiver that calls nativeInstallResult(...) — a JNI symbol exported by the app's Rust
// cdylib (Java_org_appfair_app_appfair_DayInstaller_nativeInstallResult). Also exposes the app's
// installed-version lookups (Install vs Update), and
// localized permission labels. Staged into the Gradle build via [package.metadata.day.android];
// uses day-toolkit-material's cached Context (DayBridge.ctx). No day-toolkit-material edits.
package org.appfair.app.appfair;

import android.app.PendingIntent;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.pm.PackageInfo;
import android.content.pm.PackageInstaller;
import android.content.pm.PackageManager;
import android.content.pm.PermissionInfo;
import android.os.Build;

import java.io.File;
import java.io.FileInputStream;
import java.io.InputStream;
import java.io.OutputStream;

import dev.daybrite.day.bridge.DayBridge;

public final class DayInstaller {
    private DayInstaller() {}

    /** Terminal + pending status codes, forwarded to Rust. Mirror PackageInstaller's values. */
    static native void nativeInstallResult(int status, String message);

    private static final String ACTION = "org.appfair.INSTALL_STATUS";
    private static boolean receiverRegistered = false;

    /** Launch an installed app by its package id. Returns an empty string on success, or a
     *  human-readable reason it couldn't launch (so the caller can show a dialog). */
    public static String launchApp(String pkg) {
        Context ctx = DayBridge.ctx;
        if (ctx == null) return "The app is still starting up. Try again in a moment.";
        Intent intent = ctx.getPackageManager().getLaunchIntentForPackage(pkg);
        if (intent == null) {
            return "This app has no launchable screen — it may be a service or plugin with no "
                    + "activity to open.";
        }
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        try {
            ctx.startActivity(intent);
            return "";
        } catch (Exception e) {
            return e.getMessage() == null ? e.toString() : e.getMessage();
        }
    }

    /** The device's Android API level (android.os.Build.VERSION.SDK_INT), for compatibility gating. */
    public static int deviceSdk() {
        return Build.VERSION.SDK_INT;
    }

    /** The device's supported native ABIs (Build.SUPPORTED_ABIS), tab-separated, for picking the
     *  compatible APK. */
    public static String deviceAbis() {
        StringBuilder sb = new StringBuilder();
        for (String abi : Build.SUPPORTED_ABIS) sb.append(abi).append('\t');
        return sb.toString();
    }

    private static String hex(byte[] bytes) {
        StringBuilder sb = new StringBuilder(bytes.length * 2);
        for (byte b : bytes) sb.append(String.format("%02x", b));
        return sb.toString();
    }

    /** Verify a repo's signed `entry.jar` (#1): open it with JAR signature verification, read its
     *  `entry.json` fully (which triggers verification), confirm the signing cert's SHA-256 equals
     *  `expectedFingerprint`, and return the entry.json text. Returns "" on any verification failure
     *  so the Rust side treats it as an untrusted index and refuses to sync. */
    public static String verifyEntryJar(String jarPath, String expectedFingerprint) {
        try (java.util.jar.JarFile jar = new java.util.jar.JarFile(new java.io.File(jarPath), true)) {
            java.util.jar.JarEntry entry = jar.getJarEntry("entry.json");
            if (entry == null) return "";
            byte[] data;
            try (java.io.InputStream in = jar.getInputStream(entry)) {
                java.io.ByteArrayOutputStream bos = new java.io.ByteArrayOutputStream();
                byte[] buf = new byte[8192];
                int n;
                while ((n = in.read(buf)) > 0) bos.write(buf, 0, n);
                data = bos.toByteArray();
            }
            // Certificates are only available after the entry has been fully read.
            java.security.cert.Certificate[] certs = entry.getCertificates();
            if (certs == null || certs.length == 0) return "";
            java.security.MessageDigest md = java.security.MessageDigest.getInstance("SHA-256");
            String fp = hex(md.digest(certs[0].getEncoded()));
            if (!fp.equalsIgnoreCase(expectedFingerprint)) return "";
            return new String(data, java.nio.charset.StandardCharsets.UTF_8);
        } catch (Exception e) {
            return "";
        }
    }

    /** The SHA-256 of the APK's signing certificate (#3), to compare against the catalog's pinned
     *  signer. "" when unreadable (then the Rust side skips the check). Needs API 28+. */
    public static String apkSignerSha256(String apkPath) {
        Context ctx = DayBridge.ctx;
        if (ctx == null) return "";
        try {
            PackageManager pm = ctx.getPackageManager();
            PackageInfo pi = pm.getPackageArchiveInfo(apkPath, PackageManager.GET_SIGNING_CERTIFICATES);
            if (pi == null || pi.signingInfo == null) return "";
            android.content.pm.Signature[] sigs = pi.signingInfo.getApkContentsSigners();
            if (sigs == null || sigs.length == 0) return "";
            java.security.MessageDigest md = java.security.MessageDigest.getInstance("SHA-256");
            return hex(md.digest(sigs[0].toByteArray()));
        } catch (Exception e) {
            return "";
        }
    }

    /** Register a daily background update check with WorkManager (#7), keeping any already-scheduled
     *  one. Network-constrained so it never runs offline; WorkManager persists it across reboots.
     *  Called once per launch from Rust — enqueueUniquePeriodicWork(KEEP) makes that idempotent. */
    public static void scheduleUpdateChecks() {
        Context ctx = DayBridge.ctx;
        if (ctx == null) return;
        try {
            androidx.work.Constraints constraints = new androidx.work.Constraints.Builder()
                    .setRequiredNetworkType(androidx.work.NetworkType.CONNECTED)
                    .build();
            androidx.work.PeriodicWorkRequest request =
                    new androidx.work.PeriodicWorkRequest.Builder(
                            UpdateWorker.class, 1, java.util.concurrent.TimeUnit.DAYS)
                            .setConstraints(constraints)
                            .build();
            androidx.work.WorkManager.getInstance(ctx).enqueueUniquePeriodicWork(
                    "app_fair_update_check",
                    androidx.work.ExistingPeriodicWorkPolicy.KEEP,
                    request);
        } catch (Exception ignored) {
            // WorkManager unavailable (or not initialized) — background checks are best-effort.
        }
    }

    /** Ask the system to uninstall {@code pkg} (#9). Fires the platform's uninstall confirmation
     *  (ACTION_DELETE); the user confirms, and the change shows up on the next installed-packages
     *  scan. Returns "" on success, or a human-readable reason it couldn't be started. */
    public static String uninstall(String pkg) {
        Context ctx = DayBridge.ctx;
        if (ctx == null) return "The app is still starting up. Try again in a moment.";
        try {
            Intent intent = new Intent(Intent.ACTION_DELETE)
                    .setData(android.net.Uri.parse("package:" + pkg))
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            ctx.startActivity(intent);
            return "";
        } catch (Exception e) {
            return e.getMessage() == null ? e.toString() : e.getMessage();
        }
    }

    /** Every installed app the user can see, as "pkg\tversionCode" lines — the Updates tab
     *  intersects this with the catalog to find installed + updatable apps in one pass. */
    public static String installedPackages() {
        Context ctx = DayBridge.ctx;
        if (ctx == null) return "";
        StringBuilder sb = new StringBuilder();
        for (PackageInfo info : ctx.getPackageManager().getInstalledPackages(0)) {
            // Skip system apps — an app store manages user-installed apps.
            if (info.applicationInfo != null
                    && (info.applicationInfo.flags & android.content.pm.ApplicationInfo.FLAG_SYSTEM) != 0) {
                continue;
            }
            long code = (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P)
                    ? info.getLongVersionCode()
                    : info.versionCode;
            sb.append(info.packageName).append('\t').append(code).append('\n');
        }
        return sb.toString();
    }

    /** Installed versionCode for {@code pkg}, or -1 if the app is not installed. */
    public static long installedVersion(String pkg) {
        Context ctx = DayBridge.ctx;
        if (ctx == null) return -1;
        try {
            PackageInfo info = ctx.getPackageManager().getPackageInfo(pkg, 0);
            return (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P)
                    ? info.getLongVersionCode()
                    : info.versionCode;
        } catch (PackageManager.NameNotFoundException e) {
            return -1;
        }
    }

    /** The system's localized "label\tdescription" for a permission. The label falls back to the
     *  raw name and the description is empty when the platform has neither (e.g. a custom
     *  permission the device doesn't define). Used for the tap-through info dialog. */
    public static String permissionInfo(String name) {
        Context ctx = DayBridge.ctx;
        if (ctx == null) return name + "\t";
        try {
            PackageManager pm = ctx.getPackageManager();
            PermissionInfo info = pm.getPermissionInfo(name, 0);
            CharSequence label = info.loadLabel(pm);
            CharSequence desc = info.loadDescription(pm);
            String labelStr = (label == null) ? name : label.toString();
            String descStr = (desc == null) ? "" : desc.toString();
            return labelStr + "\t" + descStr;
        } catch (Exception e) {
            return name + "\t";
        }
    }

    /** Stream the APK at {@code apkPath} into a new session and commit it. */
    public static void install(String apkPath, String appLabel) {
        Context ctx = DayBridge.ctx;
        if (ctx == null) {
            nativeInstallResult(PackageInstaller.STATUS_FAILURE, "no context");
            return;
        }
        registerReceiver(ctx);
        try {
            PackageInstaller installer = ctx.getPackageManager().getPackageInstaller();
            PackageInstaller.SessionParams params = new PackageInstaller.SessionParams(
                    PackageInstaller.SessionParams.MODE_FULL_INSTALL);
            if (appLabel != null) params.setAppLabel(appLabel);
            int sessionId = installer.createSession(params);
            PackageInstaller.Session session = installer.openSession(sessionId);
            File apk = new File(apkPath);
            try (InputStream in = new FileInputStream(apk);
                 OutputStream out = session.openWrite("app_fair", 0, apk.length())) {
                byte[] buf = new byte[65536];
                int n;
                while ((n = in.read(buf)) > 0) {
                    out.write(buf, 0, n);
                }
                session.fsync(out);
            }
            Intent intent = new Intent(ACTION).setPackage(ctx.getPackageName());
            int flags = PendingIntent.FLAG_UPDATE_CURRENT
                    | (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S ? PendingIntent.FLAG_MUTABLE : 0);
            PendingIntent pending = PendingIntent.getBroadcast(ctx, sessionId, intent, flags);
            session.commit(pending.getIntentSender());
            session.close();
        } catch (Exception e) {
            nativeInstallResult(PackageInstaller.STATUS_FAILURE,
                    e.getMessage() == null ? e.toString() : e.getMessage());
        }
    }

    private static void registerReceiver(Context context) {
        if (receiverRegistered) return;
        receiverRegistered = true;
        // Register on the APPLICATION context, not the Activity: the receiver must outlive an
        // Activity recreation (e.g. after backgrounding), or a later install's session status
        // would have no receiver and the confirm screen would never appear.
        Context ctx = context.getApplicationContext();
        BroadcastReceiver receiver = new BroadcastReceiver() {
            @Override
            public void onReceive(Context c, Intent i) {
                int status = i.getIntExtra(PackageInstaller.EXTRA_STATUS,
                        PackageInstaller.STATUS_FAILURE);
                if (status == PackageInstaller.STATUS_PENDING_USER_ACTION) {
                    Intent confirm = i.getParcelableExtra(Intent.EXTRA_INTENT);
                    if (confirm != null) {
                        confirm.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
                        try {
                            c.startActivity(confirm);
                        } catch (Exception ignored) {
                        }
                    }
                    nativeInstallResult(status, null);
                } else {
                    nativeInstallResult(status,
                            i.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE));
                }
            }
        };
        IntentFilter filter = new IntentFilter(ACTION);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            ctx.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED);
        } else {
            ctx.registerReceiver(receiver, filter);
        }
    }
}
