// Copyright © The Daybrite Project
// SPDX-License-Identifier: MPL-2.0

//! Generates the MarketplaceKit bridge's glue from the `day_bridge::bridge!` block in
//! `src/lib.rs` (https://daybrite.dev/docs/reference/bridge). Needs no Swift toolchain.
fn main() {
    day_build::bridge::generate().expect("day-build: bridge codegen");
}
