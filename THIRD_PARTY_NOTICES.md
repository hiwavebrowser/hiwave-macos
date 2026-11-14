# Third-party notices

HiWave is licensed under the Mozilla Public License 2.0 (`LICENSE`). The HiWave
application also contains the third-party works listed here, each under its own
licence. This file carries the copyright and permission notices those licences
require to travel with the binary. It must ship with every build that users
download.

Inventory taken on 2026-11-14 from `develop` at `55f08f46` (`Cargo.lock` as
committed). How to regenerate and check it is at the end of this file and in
`docs/GATES.md`.

## Contents

1. Licences with source or file-level obligations (MPL-2.0)
2. Works that are not Cargo crates (Chart.js, Unicode CLDR data)
3. Vendored source (`third_party/`: Boa)
4. Rust crates compiled into the `hiwave` binary (385 crates), by licence, with each licence text once
5. Crates under more than one licence at once, and licence files reproduced in full (dpi, unicode-ident, aws-lc-rs, aws-lc-sys, ring, libm)
6. What is in the repository but does not ship
7. How this file was made, and what was not verified

No component of the shipped binary is under a GPL, LGPL, AGPL or SSPL licence,
and none is without a licence.

## 1. Licences with source or file-level obligations

### MPL-2.0 (file-level copyleft)

- **adblock 0.12.0** (declared `MPL-2.0`). No copyright line in the published crate; Cargo.toml authors: Anton Lazarev, Andrius Aucinas. <https://github.com/brave/adblock-rust/>
- **option-ext 0.2.0** (declared `MPL-2.0`). No copyright line in the published crate; Cargo.toml authors: Simon Ochsenreither. <https://github.com/soc/option-ext.git>

What MPL-2.0 requires of us for these two crates:

- **Tell recipients where the source is** (section 3.2(a)). Both crates are used
  exactly as published on crates.io; their source is at the URLs above and at
  `https://crates.io/crates/adblock/0.12.0` and
  `https://crates.io/crates/option-ext/0.2.0`. This file is that notice.
- **Keep their files under MPL-2.0** (section 3.1). If HiWave ever changes a file
  of either crate, the changed file must be published under MPL-2.0. Vendoring a
  patched copy the way Boa is vendored would trigger this.
- **Do not restrict the recipient's rights in that source** (section 3.2(b)). The
  binary may be distributed under other terms (HiWave's own MPL-2.0, or the
  commercial licence), but those terms cannot limit what MPL-2.0 grants over
  the two crates' source.
- **Leave their notices in place** (section 3.4).

The full text of MPL-2.0 is in section 4.

## 2. Works that are not Cargo crates

### Chart.js 4.4.0 (MIT)

`crates/hiwave-app/src/ui/chart.umd.min.js`, the minified UMD build from the
`chart.js@4.4.0` npm package, embedded in the binary with `include_str!` and
injected into the built-in report page.

- **Chart.js 4.4.0**. Copyright (c) 2014-2022 Chart.js Contributors (upstream
  `LICENSE.md` at tag v4.4.0; the bundled file's own banner reads
  "(c) 2023 Chart.js Contributors, Released under the MIT License").
  <https://github.com/chartjs/Chart.js>
- **@kurkle/color 0.3.2**, bundled inside that build. Copyright (c) 2018-2021
  Jukka Kurkela (upstream `LICENSE.md` at tag v0.3.2; banner in the bundled
  file: "(c) 2023 Jukka Kurkela, Released under the MIT License").
  <https://github.com/kurkle/color>

Licence text: MIT, in section 4.

### Unicode CLDR 48 data (Unicode-3.0)

Copyright (c) 1991-2025 Unicode, Inc. All rights reserved. Distributed under
the Unicode License v3. <https://cldr.unicode.org>,
<https://github.com/unicode-org/cldr-json> (release 48.0.0),
<https://www.unicode.org/license.txt>

- `crates/rustkit-bindings/src/likely_subtags.txt`: the likely-subtags table
  (`common/supplemental/likelySubtags.xml`, read from cldr-json 48.0.0
  `cldr-core/supplemental/likelySubtags.json`). Data only, re-encoded by
  `scripts/cldr_likely_subtags.py`; embedded with `include_str!` in
  `web_intl_likely.rs`.
- `crates/rustkit-bindings/src/web_intl.js`: the first-day-of-week and weekend
  tables (`WEEK_FIRST`, `WEEKEND`, `WEEKEND_FRI_SAT`) from CLDR 48 supplemental
  `weekData`.
- The same file's English plural, list and relative-time patterns (the sections
  marked "CLDR en") follow CLDR's `en` locale data.

Licence text: Unicode License v3, in section 4.

## 3. Vendored source

`third_party/boa_engine`, `third_party/boa_parser` and `third_party/boa_ast`
are the crates.io 0.22.0 releases of the Boa JavaScript engine (upstream
`boa-dev/boa` at `337a3668`), wired in through `[patch.crates-io]`.
`boa_engine` carries one HiWave patch, described in
`third_party/boa_engine/PATCHES.md`; the other two are unmodified. Upstream
licenses them `Unlicense OR MIT`; HiWave uses them under MIT. The published
crates contain no licence file, so the notice is given here:

    Copyright (c) 2019 Jason Williams

(from `LICENSE-MIT` in <https://github.com/boa-dev/boa> at `337a3668`). They
are listed again with the other MIT crates in section 4.

## 4. Rust crates compiled into the `hiwave` binary

Every crate `cargo tree -p hiwave-app -e normal` reports for
`aarch64-apple-darwin` and `x86_64-apple-darwin`, HiWave's own crates left out.
Where a crate offers a choice of licences (`A OR B`), HiWave uses it under the
first of these that it offers: MIT, Apache-2.0, BSD-3-Clause, BSD-2-Clause, ISC, Zlib.
The expression the crate declares is shown with each entry. Copyright lines are
copied from the licence files inside each published crate; where a crate ships
none, the entry says so and gives the `authors` field of its `Cargo.toml`
instead.

### MIT License (`MIT`): 334 crates

- **addr 0.15.6** (declared `MIT/Apache-2.0`). Copyright (c) 2016 Rushmore Mushambi. <https://github.com/addr-rs/addr>
- **addr2line 0.25.1** (declared `Apache-2.0 OR MIT`). Copyright (c) 2016-2018 The gimli Developers. <https://github.com/gimli-rs/addr2line>
- **adler2 2.0.1** (declared `0BSD OR MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: Jonas Schievink, oyvindln. <https://github.com/oyvindln/adler2>
- **aead 0.5.2** (declared `MIT OR Apache-2.0`). Copyright (c) 2019 The RustCrypto Project Developers; Copyright (c) 2019 MobileCoin, LLC. <https://github.com/RustCrypto/traits>
- **aes 0.8.4** (declared `MIT OR Apache-2.0`). Copyright (c) 2018 Artyom Pavlov. <https://github.com/RustCrypto/block-ciphers>
- **aes-gcm 0.10.3** (declared `Apache-2.0 OR MIT`). Copyright (c) 2019 The RustCrypto Project Developers. <https://github.com/RustCrypto/AEADs>
- **ahash 0.8.12** (declared `MIT OR Apache-2.0`). Copyright (c) 2018 Tom Kaitchuck. <https://github.com/tkaitchuck/ahash>
- **aho-corasick 1.1.4** (declared `Unlicense OR MIT`). Copyright (c) 2015 Andrew Gallant. <https://github.com/BurntSushi/aho-corasick>
- **aligned-vec 0.6.4** (declared `MIT`). Copyright (c) 2022 sarah. <https://github.com/sarah-ek/aligned-vec/>
- **allocator-api2 0.2.21** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: Zakarum. <https://github.com/zakarumych/allocator-api2>
- **anyhow 1.0.100** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: David Tolnay. <https://github.com/dtolnay/anyhow>
- **argon2 0.5.3** (declared `MIT OR Apache-2.0`). Copyright (c) 2021-2024 The RustCrypto Project Developers. <https://github.com/RustCrypto/password-hashes/tree/master/argon2>
- **arrayvec 0.7.8** (declared `MIT OR Apache-2.0`). Copyright (c) Ulrik Sverdrup "bluss" 2015-2023. <https://github.com/bluss/arrayvec>
- **async-channel 2.5.0** (declared `Apache-2.0 OR MIT`). No copyright line in the published crate; Cargo.toml authors: Stjepan Glavina. <https://github.com/smol-rs/async-channel>
- **atomic-waker 1.1.2** (declared `Apache-2.0 OR MIT`). Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors. <https://github.com/smol-rs/atomic-waker>
- **backtrace 0.3.76** (declared `MIT OR Apache-2.0`). Copyright (c) 2014 Alex Crichton. <https://github.com/rust-lang/backtrace-rs>
- **base64 0.22.1** (declared `MIT OR Apache-2.0`). Copyright (c) 2015 Alice Maz. <https://github.com/marshallpierce/rust-base64>
- **base64ct 1.8.2** (declared `Apache-2.0 OR MIT`). Copyright (c) 2014 Steve "Sc00bz" Thomas (steve at tobtu dot com); Copyright (c) 2021-2025 The RustCrypto Project Developers. <https://github.com/RustCrypto/formats>
- **bit-set 0.8.0** (declared `Apache-2.0 OR MIT`). Copyright (c) 2023 The Rust Project Developers. <https://github.com/contain-rs/bit-set>
- **bit-vec 0.8.0** (declared `Apache-2.0 OR MIT`). Copyright (c) 2023 The Rust Project Developers. <https://github.com/contain-rs/bit-vec>
- **bitflags 1.3.2** (declared `MIT/Apache-2.0`). Copyright (c) 2014 The Rust Project Developers. <https://github.com/bitflags/bitflags>
- **bitflags 2.13.2** (declared `MIT OR Apache-2.0`). Copyright (c) 2014 The Rust Project Developers. <https://github.com/bitflags/bitflags>
- **blake2 0.10.6** (declared `MIT OR Apache-2.0`). Copyright (c) 2015-2016 The blake2-rfc Developers, Cesar Barros; Copyright (c) 2017 Artyom Pavlov. <https://github.com/RustCrypto/hashes>
- **block 0.1.6** (declared `MIT`). No copyright line in the published crate; Cargo.toml authors: Steven Sheldon. <http://github.com/SSheldon/rust-block>
- **block-buffer 0.10.4** (declared `MIT OR Apache-2.0`). Copyright (c) 2018-2019 The RustCrypto Project Developers. <https://github.com/RustCrypto/utils>
- **block2 0.6.2** (declared `MIT`). No copyright line in the published crate; Cargo.toml authors: Mads Marquart. <https://github.com/madsmtm/objc2>
- **boa_ast 0.22.0** (declared `Unlicense OR MIT`). Copyright (c) 2019 Jason Williams (from LICENSE-MIT in boa-dev/boa at 337a3668; the published crate carries no licence file). <https://github.com/boa-dev/boa> [vendored in `third_party/boa_ast`, unmodified]
- **boa_engine 0.22.0** (declared `Unlicense OR MIT`). Copyright (c) 2019 Jason Williams (from LICENSE-MIT in boa-dev/boa at 337a3668; the published crate carries no licence file). <https://github.com/boa-dev/boa> [vendored in `third_party/boa_engine`, patched (see `third_party/boa_engine/PATCHES.md`)]
- **boa_gc 0.22.0** (declared `Unlicense OR MIT`). Copyright (c) 2019 Jason Williams (from LICENSE-MIT in boa-dev/boa at 337a3668; the published crate carries no licence file). <https://github.com/boa-dev/boa>
- **boa_interner 0.22.0** (declared `Unlicense OR MIT`). Copyright (c) 2019 Jason Williams (from LICENSE-MIT in boa-dev/boa at 337a3668; the published crate carries no licence file). <https://github.com/boa-dev/boa>
- **boa_macros 0.22.0** (declared `Unlicense OR MIT`). Copyright (c) 2019 Jason Williams (from LICENSE-MIT in boa-dev/boa at 337a3668; the published crate carries no licence file). <https://github.com/boa-dev/boa> [procedural macro: runs at build time, its expansion is compiled in]
- **boa_parser 0.22.0** (declared `Unlicense OR MIT`). Copyright (c) 2019 Jason Williams (from LICENSE-MIT in boa-dev/boa at 337a3668; the published crate carries no licence file). <https://github.com/boa-dev/boa> [vendored in `third_party/boa_parser`, unmodified]
- **boa_string 0.22.0** (declared `Unlicense OR MIT`). Copyright (c) 2019 Jason Williams (from LICENSE-MIT in boa-dev/boa at 337a3668; the published crate carries no licence file). <https://github.com/boa-dev/boa>
- **brotli-decompressor 6.1.0** (declared `BSD-3-Clause/MIT`). Copyright (c) 2016 Dropbox, Inc. <https://github.com/dropbox/rust-brotli-decompressor>
- **bytemuck 1.25.2** (declared `Zlib OR Apache-2.0 OR MIT`). Copyright (c) 2019 Daniel "Lokathor" Gee. <https://github.com/Lokathor/bytemuck>
- **bytemuck_derive 1.10.2** (declared `Zlib OR Apache-2.0 OR MIT`). Copyright (c) 2019 Daniel "Lokathor" Gee. <https://github.com/Lokathor/bytemuck> [procedural macro: runs at build time, its expansion is compiled in]
- **byteorder 1.5.0** (declared `Unlicense OR MIT`). Copyright (c) 2015 Andrew Gallant. <https://github.com/BurntSushi/byteorder>
- **byteorder-lite 0.1.0** (declared `Unlicense OR MIT`). Copyright (c) 2015 Andrew Gallant. <https://github.com/image-rs/byteorder-lite>
- **bytes 1.11.0** (declared `MIT`). Copyright (c) 2018 Carl Lerche. <https://github.com/tokio-rs/bytes>
- **cfg-if 1.0.4** (declared `MIT OR Apache-2.0`). Copyright (c) 2014 Alex Crichton. <https://github.com/rust-lang/cfg-if>
- **chacha20 0.10.2** (declared `MIT OR Apache-2.0`). Copyright (c) 2019-2026 The RustCrypto Project Developers. <https://github.com/RustCrypto/stream-ciphers>
- **chrono 0.4.42** (declared `MIT OR Apache-2.0`). Copyright (c) 2014, Kang Seonghoon. <https://github.com/chronotope/chrono>
- **cipher 0.4.4** (declared `MIT OR Apache-2.0`). Copyright (c) 2016-2020 RustCrypto Developers. <https://github.com/RustCrypto/traits>
- **cocoa 0.25.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2012-2013 Mozilla Foundation. <https://github.com/servo/core-foundation-rs>
- **cocoa 0.26.1** (declared `MIT OR Apache-2.0`). Copyright (c) 2012-2013 Mozilla Foundation. <https://github.com/servo/core-foundation-rs>
- **cocoa-foundation 0.1.2** (declared `MIT OR Apache-2.0`). Copyright (c) 2012-2013 Mozilla Foundation. <https://github.com/servo/core-foundation-rs>
- **cocoa-foundation 0.2.1** (declared `MIT OR Apache-2.0`). Copyright (c) 2012-2013 Mozilla Foundation. <https://github.com/servo/core-foundation-rs>
- **color_quant 1.1.0** (declared `MIT`). Copyright (c) 2016 PistonDevelopers. <https://github.com/image-rs/color_quant.git>
- **concurrent-queue 2.5.0** (declared `Apache-2.0 OR MIT`). No copyright line in the published crate; Cargo.toml authors: Stjepan Glavina, Taiki Endo, John Nunley. <https://github.com/smol-rs/concurrent-queue>
- **cookie 0.18.1** (declared `MIT OR Apache-2.0`). Copyright 2017 Sergio Benitez; Copyright 2014 Alex Chricton; Copyright (c) 2017 Sergio Benitez; Copyright (c) 2014 Alex Crichton. <https://github.com/SergioBenitez/cookie-rs>
- **core-foundation 0.10.1** (declared `MIT OR Apache-2.0`). Copyright (c) 2012-2013 Mozilla Foundation. <https://github.com/servo/core-foundation-rs>
- **core-foundation 0.9.4** (declared `MIT OR Apache-2.0`). Copyright (c) 2012-2013 Mozilla Foundation. <https://github.com/servo/core-foundation-rs>
- **core-foundation-sys 0.8.7** (declared `MIT OR Apache-2.0`). Copyright (c) 2012-2013 Mozilla Foundation. <https://github.com/servo/core-foundation-rs>
- **core-graphics 0.23.2** (declared `MIT OR Apache-2.0`). Copyright (c) 2012-2013 Mozilla Foundation. <https://github.com/servo/core-foundation-rs>
- **core-graphics 0.24.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2012-2013 Mozilla Foundation. <https://github.com/servo/core-foundation-rs>
- **core-graphics-types 0.1.3** (declared `MIT OR Apache-2.0`). Copyright (c) 2012-2013 Mozilla Foundation. <https://github.com/servo/core-foundation-rs>
- **core-graphics-types 0.2.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2012-2013 Mozilla Foundation. <https://github.com/servo/core-foundation-rs>
- **core-text 20.1.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2012-2013 Mozilla Foundation. <https://github.com/servo/core-foundation-rs>
- **core_maths 0.1.1** (declared `MIT`). Copyright (c) 2024 Robert Bastian. <https://github.com/robertbastian/core_maths>
- **cow-utils 0.1.3** (declared `MIT`). Copyright (c) 2020 Ingvar Stepanyan. <https://github.com/RReverser/cow-utils-rs>
- **cpufeatures 0.2.17** (declared `MIT OR Apache-2.0`). Copyright (c) 2020-2025 The RustCrypto Project Developers. <https://github.com/RustCrypto/utils>
- **cpufeatures 0.3.1** (declared `MIT OR Apache-2.0`). Copyright (c) 2020-2026 The RustCrypto Project Developers. <https://github.com/RustCrypto/utils> [x86_64 builds only]
- **crc32fast 1.5.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2018 Sam Rijs, Alex Crichton and contributors. <https://github.com/srijs/rust-crc32fast>
- **crossbeam-channel 0.5.15** (declared `MIT OR Apache-2.0`). Copyright (c) 2019 The Crossbeam Project Developers; Copyright (c) 2009 The Go Authors. All rights reserved. <https://github.com/crossbeam-rs/crossbeam>
- **crossbeam-deque 0.8.6** (declared `MIT OR Apache-2.0`). Copyright (c) 2019 The Crossbeam Project Developers. <https://github.com/crossbeam-rs/crossbeam>
- **crossbeam-epoch 0.9.18** (declared `MIT OR Apache-2.0`). Copyright (c) 2019 The Crossbeam Project Developers. <https://github.com/crossbeam-rs/crossbeam>
- **crossbeam-utils 0.8.21** (declared `MIT OR Apache-2.0`). Copyright (c) 2019 The Crossbeam Project Developers. <https://github.com/crossbeam-rs/crossbeam>
- **crypto-common 0.1.7** (declared `MIT OR Apache-2.0`). Copyright (c) 2021 RustCrypto Developers. <https://github.com/RustCrypto/traits>
- **ctr 0.9.2** (declared `MIT OR Apache-2.0`). Copyright (c) 2018-2022 RustCrypto Developers; Copyright (c) 2018 Artyom Pavlov. <https://github.com/RustCrypto/block-modes>
- **dashmap 6.2.1** (declared `MIT`). Copyright (c) 2019 Acrimon. <https://github.com/xacrimon/dashmap>
- **deranged 0.5.8** (declared `MIT OR Apache-2.0`). Copyright 2024 Jacob Pratt et al; Copyright (c) 2024 Jacob Pratt et al. <https://github.com/jhpratt/deranged>
- **digest 0.10.7** (declared `MIT OR Apache-2.0`). Copyright (c) 2017 Artyom Pavlov. <https://github.com/RustCrypto/traits>
- **dirs 5.0.1** (declared `MIT OR Apache-2.0`). Copyright (c) 2018-2019 dirs-rs contributors. <https://github.com/soc/dirs-rs>
- **dirs-sys 0.4.1** (declared `MIT OR Apache-2.0`). Copyright (c) 2018-2019 dirs-rs contributors. <https://github.com/dirs-dev/dirs-sys-rs>
- **dispatch 0.2.0** (declared `MIT`). No copyright line in the published crate; Cargo.toml authors: Steven Sheldon. <http://github.com/SSheldon/rust-dispatch>
- **dispatch2 0.3.0** (declared `Zlib OR Apache-2.0 OR MIT`). No copyright line in the published crate; Cargo.toml authors: Mads Marquart, Mary. <https://github.com/madsmtm/objc2>
- **displaydoc 0.2.5** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: Jane Lusby. <https://github.com/yaahc/displaydoc> [procedural macro: runs at build time, its expansion is compiled in]
- **document-features 0.2.12** (declared `MIT OR Apache-2.0`). Copyright (c) 2020 Olivier Goffart <ogoffart@sixtyfps.io>. <https://github.com/slint-ui/document-features> [procedural macro: runs at build time, its expansion is compiled in]
- **dynify 0.1.2** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: Loi Chyan. <https://github.com/loichyan/dynify>
- **dynify-macros 0.1.2** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: Loi Chyan. <https://github.com/loichyan/dynify> [procedural macro: runs at build time, its expansion is compiled in]
- **either 1.15.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2015. <https://github.com/rayon-rs/either>
- **equator 0.4.2** (declared `MIT`). Copyright (c) 2023 sarah. <https://github.com/sarah-ek/equator/>
- **equator-macro 0.4.2** (declared `MIT`). Copyright (c) 2023 sarah. <https://github.com/sarah-ek/equator/> [procedural macro: runs at build time, its expansion is compiled in]
- **equivalent 1.0.2** (declared `Apache-2.0 OR MIT`). Copyright (c) 2016--2023. <https://github.com/indexmap-rs/equivalent>
- **event-listener 5.4.1** (declared `Apache-2.0 OR MIT`). No copyright line in the published crate; Cargo.toml authors: Stjepan Glavina, John Nunley. <https://github.com/smol-rs/event-listener>
- **event-listener-strategy 0.5.4** (declared `Apache-2.0 OR MIT`). No copyright line in the published crate; Cargo.toml authors: John Nunley. <https://github.com/smol-rs/event-listener-strategy>
- **fallible-iterator 0.3.0** (declared `MIT/Apache-2.0`). Copyright (c) 2015 The rust-openssl-verify Developers. <https://github.com/sfackler/rust-fallible-iterator>
- **fallible-streaming-iterator 0.1.9** (declared `MIT/Apache-2.0`). Copyright (c) 2016 The fallible-streaming-iterator Developers. <https://github.com/sfackler/fallible-streaming-iterator>
- **fast-float2 0.2.4** (declared `MIT OR Apache-2.0`). Copyright 2021 Ivan Smirnov. <https://github.com/Alexhuszagh/fast-float-rust>
- **fastrand 2.3.0** (declared `Apache-2.0 OR MIT`). No copyright line in the published crate; Cargo.toml authors: Stjepan Glavina. <https://github.com/smol-rs/fastrand>
- **fdeflate 0.3.7** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: The image-rs Developers. <https://github.com/image-rs/fdeflate>
- **fixedbitset 0.5.7** (declared `MIT OR Apache-2.0`). Copyright (c) 2015-2017. <https://github.com/petgraph/fixedbitset>
- **flate2 1.1.5** (declared `MIT OR Apache-2.0`). Copyright (c) 2014-2025 Alex Crichton. <https://github.com/rust-lang/flate2-rs>
- **float16 0.1.7** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: Kathryn Long, Alex Huszagh. <https://github.com/Alexhuszagh/float16>
- **fnv 1.0.7** (declared `Apache-2.0 / MIT`). Copyright (c) 2017 Contributors. <https://github.com/servo/rust-fnv>
- **foreign-types 0.5.0** (declared `MIT/Apache-2.0`). Copyright (c) 2017 The foreign-types Developers. <https://github.com/sfackler/foreign-types>
- **foreign-types-macros 0.2.3** (declared `MIT/Apache-2.0`). Copyright (c) 2017 The foreign-types Developers. <https://github.com/sfackler/foreign-types> [procedural macro: runs at build time, its expansion is compiled in]
- **foreign-types-shared 0.3.1** (declared `MIT/Apache-2.0`). Copyright (c) 2017 The foreign-types Developers. <https://github.com/sfackler/foreign-types>
- **form_urlencoded 1.2.2** (declared `MIT OR Apache-2.0`). Copyright (c) 2013-2016 The rust-url developers. <https://github.com/servo/rust-url>
- **futures 0.3.31** (declared `MIT OR Apache-2.0`). Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors. <https://github.com/rust-lang/futures-rs>
- **futures-channel 0.3.31** (declared `MIT OR Apache-2.0`). Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors. <https://github.com/rust-lang/futures-rs>
- **futures-concurrency 7.7.1** (declared `MIT OR Apache-2.0`). Copyright 2020 Yoshua Wuyts; Copyright (c) 2020 Yoshua Wuyts. <https://github.com/yoshuawuyts/futures-concurrency>
- **futures-core 0.3.31** (declared `MIT OR Apache-2.0`). Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors. <https://github.com/rust-lang/futures-rs>
- **futures-executor 0.3.31** (declared `MIT OR Apache-2.0`). Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors. <https://github.com/rust-lang/futures-rs>
- **futures-io 0.3.31** (declared `MIT OR Apache-2.0`). Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors. <https://github.com/rust-lang/futures-rs>
- **futures-lite 2.6.1** (declared `Apache-2.0 OR MIT`). Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors. <https://github.com/smol-rs/futures-lite>
- **futures-macro 0.3.31** (declared `MIT OR Apache-2.0`). Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors. <https://github.com/rust-lang/futures-rs> [procedural macro: runs at build time, its expansion is compiled in]
- **futures-sink 0.3.31** (declared `MIT OR Apache-2.0`). Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors. <https://github.com/rust-lang/futures-rs>
- **futures-task 0.3.31** (declared `MIT OR Apache-2.0`). Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors. <https://github.com/rust-lang/futures-rs>
- **futures-util 0.3.31** (declared `MIT OR Apache-2.0`). Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors. <https://github.com/rust-lang/futures-rs>
- **fuzzy-matcher 0.3.7** (declared `MIT`). Copyright (c) 2019 Jinzhou Zhang. <https://github.com/lotabout/fuzzy-matcher>
- **generic-array 0.14.7** (declared `MIT`). Copyright (c) 2015 Bartłomiej Kamiński. <https://github.com/fizyk20/generic-array.git>
- **getrandom 0.2.16** (declared `MIT OR Apache-2.0`). Copyright (c) 2018-2024 The rust-random Project Developers; Copyright (c) 2014 The Rust Project Developers. <https://github.com/rust-random/getrandom>
- **getrandom 0.4.3** (declared `MIT OR Apache-2.0`). Copyright (c) 2018-2026 The rust-random Project Developers; Copyright (c) 2014 The Rust Project Developers. <https://github.com/rust-random/getrandom>
- **ghash 0.5.1** (declared `Apache-2.0 OR MIT`). Copyright (c) 2019 RustCrypto Developers. <https://github.com/RustCrypto/universal-hashes>
- **gif 0.13.3** (declared `MIT OR Apache-2.0`). Copyright (c) 2015 nwin. <https://github.com/image-rs/image-gif>
- **gimli 0.32.3** (declared `MIT OR Apache-2.0`). Copyright (c) 2015 The Rust Project Developers. <https://github.com/gimli-rs/gimli>
- **h2 0.4.12** (declared `MIT`). Copyright (c) 2017 h2 authors. <https://github.com/hyperium/h2>
- **hashbrown 0.14.5** (declared `MIT OR Apache-2.0`). Copyright (c) 2016 Amanieu d'Antras. <https://github.com/rust-lang/hashbrown>
- **hashbrown 0.15.5** (declared `MIT OR Apache-2.0`). Copyright (c) 2016 Amanieu d'Antras. <https://github.com/rust-lang/hashbrown>
- **hashbrown 0.17.1** (declared `MIT OR Apache-2.0`). Copyright (c) 2016 Amanieu d'Antras. <https://github.com/rust-lang/hashbrown>
- **hashlink 0.9.1** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: kyren. <https://github.com/kyren/hashlink>
- **heck 0.5.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2015 The Rust Project Developers. <https://github.com/withoutboats/heck>
- **http 1.4.0** (declared `MIT OR Apache-2.0`). Copyright 2017 http-rs authors; Copyright (c) 2017 http-rs authors. <https://github.com/hyperium/http>
- **http-body 1.0.1** (declared `MIT`). Copyright (c) 2019-2024 Sean McArthur & Hyper Contributors. <https://github.com/hyperium/http-body>
- **http-body-util 0.1.3** (declared `MIT`). Copyright (c) 2019-2025 Sean McArthur & Hyper Contributors. <https://github.com/hyperium/http-body>
- **httparse 1.10.1** (declared `MIT OR Apache-2.0`). Copyright (c) 2015-2025 Sean McArthur. <https://github.com/seanmonstar/httparse>
- **httpdate 1.0.3** (declared `MIT OR Apache-2.0`). Copyright (c) 2016 Pyfisch. <https://github.com/pyfisch/httpdate>
- **hyper 1.8.1** (declared `MIT`). Copyright (c) 2014-2025 Sean McArthur. <https://github.com/hyperium/hyper>
- **hyper-rustls 0.27.7** (declared `Apache-2.0 OR ISC OR MIT`). Copyright (c) 2016, Joseph Birr-Pixton <jpixton@gmail.com>; Copyright (c) 2016 Joseph Birr-Pixton <jpixton@gmail.com>. <https://github.com/rustls/hyper-rustls>
- **hyper-util 0.1.19** (declared `MIT`). Copyright (c) 2023-2025 Sean McArthur. <https://github.com/hyperium/hyper-util>
- **iana-time-zone 0.1.64** (declared `MIT OR Apache-2.0`). Copyright 2020 Andrew Straw; Copyright (c) 2020 Andrew D. Straw. <https://github.com/strawlab/iana-time-zone>
- **idna 1.1.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2013-2025 The rust-url developers. <https://github.com/servo/rust-url/>
- **idna_adapter 1.2.1** (declared `Apache-2.0 OR MIT`). Copyright (c) The rust-url developers. <https://github.com/hsivonen/idna_adapter>
- **image-webp 0.2.4** (declared `MIT OR Apache-2.0`). No copyright line and no authors field in the published crate. <https://github.com/image-rs/image-webp>
- **indexmap 2.14.2** (declared `Apache-2.0 OR MIT`). Copyright (c) 2016--2017. <https://github.com/indexmap-rs/indexmap>
- **inout 0.1.4** (declared `MIT OR Apache-2.0`). Copyright (c) 2022 The RustCrypto Project Developers; Copyright (c) 2022 Artyom Pavlov. <https://github.com/RustCrypto/utils>
- **intrusive-collections 0.10.3** (declared `MIT OR Apache-2.0`). Copyright (c) 2016 Amanieu d'Antras. <https://github.com/Amanieu/intrusive-rs>
- **ipnet 2.11.0** (declared `MIT OR Apache-2.0`). Copyright 2017 Juniper Networks, Inc. <https://github.com/krisprice/ipnet>
- **iri-string 0.7.10** (declared `MIT OR Apache-2.0`). Copyright 2019-2024 YOSHIOKA Takuma. <https://github.com/lo48576/iri-string>
- **itertools 0.13.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2015. <https://github.com/rust-itertools/itertools>
- **itertools 0.15.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2015. <https://github.com/rust-itertools/itertools>
- **itoa 1.0.18** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: David Tolnay. <https://github.com/dtolnay/itoa>
- **jpeg-decoder 0.3.2** (declared `MIT OR Apache-2.0`). Copyright (c) 2016 The jpeg-decoder Developers. <https://github.com/image-rs/jpeg-decoder>
- **keyboard-types 0.7.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2017 Pyfisch. <https://github.com/pyfisch/keyboard-types>
- **lazy_static 1.5.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2010 The Rust Project Developers. <https://github.com/rust-lang-nursery/lazy-static.rs>
- **libc 0.2.179** (declared `MIT OR Apache-2.0`). Copyright (c) 2014-2020 The Rust Project Developers. <https://github.com/rust-lang/libc>
- **libm 0.2.16** (declared `MIT`). Copyright (c) 2018 Jorge Aparicio, and the other holders named in its licence files, which are reproduced in full in section 5. <https://github.com/rust-lang/compiler-builtins>
- **libsqlite3-sys 0.30.1** (declared `MIT`). Copyright (c) 2014-2021 The rusqlite developers. <https://github.com/rusqlite/rusqlite>
- **litrs 1.0.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2020 Project Developers. <https://github.com/LukasKalbertodt/litrs>
- **lock_api 0.4.14** (declared `MIT OR Apache-2.0`). Copyright (c) 2016 The Rust Project Developers. <https://github.com/Amanieu/parking_lot>
- **log 0.4.29** (declared `MIT OR Apache-2.0`). Copyright (c) 2014 The Rust Project Developers. <https://github.com/rust-lang/log>
- **lru 0.12.5** (declared `MIT`). Copyright (c) 2016 Jerome Froelich. <https://github.com/jeromefroe/lru-rs.git>
- **malloc_buf 0.0.6** (declared `MIT`). No copyright line in the published crate; Cargo.toml authors: Steven Sheldon. <https://github.com/SSheldon/malloc_buf>
- **matchers 0.2.0** (declared `MIT`). Copyright (c) 2019 Eliza Weisman. <https://github.com/hawkw/matchers>
- **memchr 2.7.6** (declared `Unlicense OR MIT`). Copyright (c) 2015 Andrew Gallant. <https://github.com/BurntSushi/memchr>
- **metal 0.29.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2010 The Rust Project Developers. <https://github.com/gfx-rs/metal-rs>
- **metal 0.31.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2010 The Rust Project Developers. <https://github.com/gfx-rs/metal-rs>
- **mime 0.3.17** (declared `MIT OR Apache-2.0`). Copyright (c) 2014 Sean McArthur. <https://github.com/hyperium/mime>
- **mime_guess 2.0.5** (declared `MIT`). Copyright (c) 2015 Austin Bonander. <https://github.com/abonander/mime_guess>
- **miniz_oxide 0.8.9** (declared `MIT OR Zlib OR Apache-2.0`). Copyright 2013-2014 RAD Game Tools and Valve Software; Copyright 2010-2014 Rich Geldreich and Tenacious Software LLC; Copyright (c) 2017 Frommi; Copyright (c) 2017-2024 oyvindln; Copyright (c) 2020 Frommi. <https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide>
- **mio 1.1.1** (declared `MIT`). Copyright (c) 2014 Carl Lerche and other MIO contributors. <https://github.com/tokio-rs/mio>
- **muda 0.15.3** (declared `Apache-2.0 OR MIT`). Copyright (c) 2022-2022 Tauri Programme within The Commons Conservancy. <https://github.com/amrbashir/muda>
- **naga 23.1.0** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: gfx-rs developers. <https://github.com/gfx-rs/wgpu/tree/trunk/naga>
- **naga 24.0.0** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: gfx-rs developers. <https://github.com/gfx-rs/wgpu/tree/trunk/naga>
- **new_debug_unreachable 1.0.6** (declared `MIT`). Copyright (c) 2015 Jonathan Reem. <https://github.com/mbrubeck/rust-debug-unreachable>
- **nu-ansi-term 0.50.3** (declared `MIT`). Copyright (c) 2014 Benjamin Sago; Copyright (c) 2021-2022 The Nushell Project Developers. <https://github.com/nushell/nu-ansi-term>
- **num-bigint 0.5.1** (declared `MIT OR Apache-2.0`). Copyright (c) 2014 The Rust Project Developers. <https://github.com/rust-num/num-bigint>
- **num-conv 0.2.2** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: Jacob Pratt. <https://github.com/jhpratt/num-conv>
- **num-integer 0.1.47** (declared `MIT OR Apache-2.0`). Copyright (c) 2014 The Rust Project Developers. <https://github.com/rust-num/num-integer>
- **num-traits 0.2.19** (declared `MIT OR Apache-2.0`). Copyright (c) 2014 The Rust Project Developers. <https://github.com/rust-num/num-traits>
- **num_cpus 1.17.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2015-2025 Sean McArthur. <https://github.com/seanmonstar/num_cpus>
- **num_enum 0.7.6** (declared `BSD-3-Clause OR MIT OR Apache-2.0`). Copyright (c) 2018, Daniel Wagner-Hall. <https://github.com/illicitonion/num_enum>
- **num_enum_derive 0.7.6** (declared `BSD-3-Clause OR MIT OR Apache-2.0`). Copyright (c) 2018, Daniel Wagner-Hall. <https://github.com/illicitonion/num_enum> [procedural macro: runs at build time, its expansion is compiled in]
- **num_threads 0.1.7** (declared `MIT OR Apache-2.0`). Copyright 2021 Jacob Pratt; Copyright (c) 2021 Jacob Pratt. <https://github.com/jhpratt/num_threads>
- **objc 0.2.7** (declared `MIT`). No copyright line in the published crate; Cargo.toml authors: Steven Sheldon. <http://github.com/SSheldon/rust-objc>
- **objc-sys 0.3.5** (declared `MIT`). No copyright line in the published crate; Cargo.toml authors: Mads Marquart. <https://github.com/madsmtm/objc2>
- **objc2 0.5.2** (declared `MIT`). No copyright line in the published crate; Cargo.toml authors: Steven Sheldon, Mads Marquart. <https://github.com/madsmtm/objc2>
- **objc2 0.6.3** (declared `MIT`). No copyright line in the published crate; Cargo.toml authors: Mads Marquart. <https://github.com/madsmtm/objc2>
- **objc2-app-kit 0.2.2** (declared `MIT`). No copyright line and no authors field in the published crate. <https://github.com/madsmtm/objc2>
- **objc2-app-kit 0.3.2** (declared `Zlib OR Apache-2.0 OR MIT`). No copyright line and no authors field in the published crate. <https://github.com/madsmtm/objc2>
- **objc2-core-foundation 0.3.2** (declared `Zlib OR Apache-2.0 OR MIT`). No copyright line and no authors field in the published crate. <https://github.com/madsmtm/objc2>
- **objc2-encode 4.1.0** (declared `MIT`). No copyright line in the published crate; Cargo.toml authors: Mads Marquart. <https://github.com/madsmtm/objc2>
- **objc2-exception-helper 0.1.1** (declared `Zlib OR Apache-2.0 OR MIT`). No copyright line in the published crate; Cargo.toml authors: Mads Marquart. <https://github.com/madsmtm/objc2>
- **objc2-foundation 0.2.2** (declared `MIT`). No copyright line and no authors field in the published crate. <https://github.com/madsmtm/objc2>
- **objc2-foundation 0.3.2** (declared `MIT`). No copyright line and no authors field in the published crate. <https://github.com/madsmtm/objc2>
- **objc2-web-kit 0.3.2** (declared `Zlib OR Apache-2.0 OR MIT`). No copyright line and no authors field in the published crate. <https://github.com/madsmtm/objc2>
- **object 0.37.3** (declared `Apache-2.0 OR MIT`). Copyright (c) 2015 The Gimli Developers. <https://github.com/gimli-rs/object>
- **once_cell 1.21.4** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: Aleksey Kladov. <https://github.com/matklad/once_cell>
- **oneshot 0.2.1** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: Linus Färnstrand. <https://github.com/faern/oneshot>
- **opaque-debug 0.3.1** (declared `MIT OR Apache-2.0`). Copyright (c) 2018-2024 The RustCrypto Project Developers. <https://github.com/RustCrypto/utils>
- **parking 2.2.1** (declared `Apache-2.0 OR MIT`). Copyright 2014-2020 The Rust Project Developers. <https://github.com/smol-rs/parking>
- **parking_lot 0.12.5** (declared `MIT OR Apache-2.0`). Copyright (c) 2016 The Rust Project Developers. <https://github.com/Amanieu/parking_lot>
- **parking_lot_core 0.9.12** (declared `MIT OR Apache-2.0`). Copyright (c) 2016 The Rust Project Developers. <https://github.com/Amanieu/parking_lot>
- **password-hash 0.5.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2020-2023 RustCrypto Developers. <https://github.com/RustCrypto/traits/tree/master/password-hash>
- **paste 1.0.15** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: David Tolnay. <https://github.com/dtolnay/paste> [procedural macro: runs at build time, its expansion is compiled in]
- **pastey 0.2.3** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: Aditya Kumar, David Tolnay. <https://github.com/as1100k/pastey> [procedural macro: runs at build time, its expansion is compiled in]
- **percent-encoding 2.3.2** (declared `MIT OR Apache-2.0`). Copyright (c) 2013-2025 The rust-url developers. <https://github.com/servo/rust-url/>
- **phf 0.14.0** (declared `MIT`). Copyright (c) 2014-2022 Steven Fackler, Yuki Okushi. <https://github.com/rust-phf/rust-phf>
- **phf_generator 0.14.0** (declared `MIT`). Copyright (c) 2014-2022 Steven Fackler, Yuki Okushi. <https://github.com/rust-phf/rust-phf>
- **phf_macros 0.14.0** (declared `MIT`). Copyright (c) 2014-2022 Steven Fackler, Yuki Okushi. <https://github.com/rust-phf/rust-phf> [procedural macro: runs at build time, its expansion is compiled in]
- **phf_shared 0.11.3** (declared `MIT`). Copyright (c) 2014-2022 Steven Fackler, Yuki Okushi. <https://github.com/rust-phf/rust-phf>
- **phf_shared 0.14.0** (declared `MIT`). Copyright (c) 2014-2022 Steven Fackler, Yuki Okushi. <https://github.com/rust-phf/rust-phf>
- **pin-project 1.1.13** (declared `Apache-2.0 OR MIT`). No copyright line and no authors field in the published crate. <https://github.com/taiki-e/pin-project>
- **pin-project-internal 1.1.13** (declared `Apache-2.0 OR MIT`). No copyright line and no authors field in the published crate. <https://github.com/taiki-e/pin-project> [procedural macro: runs at build time, its expansion is compiled in]
- **pin-project-lite 0.2.16** (declared `Apache-2.0 OR MIT`). No copyright line and no authors field in the published crate. <https://github.com/taiki-e/pin-project-lite>
- **pin-utils 0.1.0** (declared `MIT OR Apache-2.0`). Copyright 2018 The pin-utils authors; Copyright (c) 2018 The pin-utils authors. <https://github.com/rust-lang-nursery/pin-utils>
- **png 0.17.16** (declared `MIT OR Apache-2.0`). Copyright (c) 2015 nwin. <https://github.com/image-rs/image-png>
- **pollster 0.4.0** (declared `Apache-2.0/MIT`). Copyright (c) 2020-2021 Joshua Barretto. <https://github.com/zesterer/pollster>
- **polyval 0.6.2** (declared `Apache-2.0 OR MIT`). Copyright (c) 2019-2023 RustCrypto Developers. <https://github.com/RustCrypto/universal-hashes>
- **portable-atomic 1.15.0** (declared `Apache-2.0 OR MIT`). No copyright line and no authors field in the published crate. <https://github.com/taiki-e/portable-atomic>
- **powerfmt 0.2.0** (declared `MIT OR Apache-2.0`). Copyright 2023 Jacob Pratt et al; Copyright (c) 2023 Jacob Pratt et al. <https://github.com/jhpratt/powerfmt>
- **ppv-lite86 0.2.21** (declared `MIT OR Apache-2.0`). Copyright 2019 The CryptoCorrosion Contributors; Copyright (c) 2019 The CryptoCorrosion Contributors. <https://github.com/cryptocorrosion/cryptocorrosion>
- **precomputed-hash 0.1.1** (declared `MIT`). Copyright (c) 2017 Emilio Cobos Álvarez. <https://github.com/emilio/precomputed-hash>
- **proc-macro-crate 3.4.0** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: Bastian Köcher. <https://github.com/bkchr/proc-macro-crate>
- **proc-macro2 1.0.104** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: David Tolnay, Alex Crichton. <https://github.com/dtolnay/proc-macro2>
- **profiling 1.0.17** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: Philip Degarmo. <https://github.com/aclysma/profiling>
- **psl 2.1.175** (declared `MIT/Apache-2.0`). Copyright (c) 2016 Rushmore Mushambi. <https://github.com/addr-rs/psl>
- **psl-types 2.0.11** (declared `MIT/Apache-2.0`). Copyright (c) 2021 Rushmore Mushambi. <https://github.com/addr-rs/psl-types>
- **quick-error 2.0.1** (declared `MIT/Apache-2.0`). Copyright (c) 2015 The quick-error Developers. <http://github.com/tailhook/quick-error>
- **quote 1.0.47** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: David Tolnay. <https://github.com/dtolnay/quote>
- **rand 0.10.3** (declared `MIT OR Apache-2.0`). copyright assignment is required to contribute to the Rand project; Copyright 2018 Developers of the Rand project; Copyright (c) 2014 The Rust Project Developers. <https://github.com/rust-random/rand>
- **rand 0.8.5** (declared `MIT OR Apache-2.0`). copyright assignment is required to contribute to the Rand project; Copyright 2018 Developers of the Rand project; Copyright (c) 2014 The Rust Project Developers. <https://github.com/rust-random/rand>
- **rand_chacha 0.3.1** (declared `MIT OR Apache-2.0`). copyright assignment is required to contribute to the Rand project; Copyright 2018 Developers of the Rand project; Copyright (c) 2014 The Rust Project Developers. <https://github.com/rust-random/rand>
- **rand_core 0.10.1** (declared `MIT OR Apache-2.0`). copyright assignment is required to contribute to the Rand project; Copyright (c) 2018-2026 The Rand Project Developers. <https://github.com/rust-random/rand_core>
- **rand_core 0.6.4** (declared `MIT OR Apache-2.0`). copyright assignment is required to contribute to the Rand project; Copyright 2018 Developers of the Rand project; Copyright (c) 2014 The Rust Project Developers. <https://github.com/rust-random/rand>
- **raw-window-handle 0.6.2** (declared `MIT OR Apache-2.0 OR Zlib`). Copyright (c) 2019 Osspial; Copyright (c) 2020 Osspial. <https://github.com/rust-windowing/raw-window-handle>
- **rayon 1.11.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2010 The Rust Project Developers. <https://github.com/rayon-rs/rayon>
- **rayon-core 1.13.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2010 The Rust Project Developers. <https://github.com/rayon-rs/rayon>
- **regex 1.12.2** (declared `MIT OR Apache-2.0`). Copyright (c) 2014 The Rust Project Developers. <https://github.com/rust-lang/regex>
- **regex-automata 0.4.13** (declared `MIT OR Apache-2.0`). Copyright (c) 2014 The Rust Project Developers. <https://github.com/rust-lang/regex>
- **regex-syntax 0.8.8** (declared `MIT OR Apache-2.0`). Copyright (c) 2014 The Rust Project Developers. <https://github.com/rust-lang/regex>
- **regress 0.12.0** (declared `MIT OR Apache-2.0`). Copyright 2020 ridiculous_fish; Copyright (c) 2020 ridiculous_fish. <https://github.com/ridiculousfish/regress>
- **reqwest 0.12.28** (declared `MIT OR Apache-2.0`). Copyright 2016 Sean McArthur; Copyright (c) 2016-2025 Sean McArthur. <https://github.com/seanmonstar/reqwest>
- **rfd 0.15.4** (declared `MIT`). Copyright (c) 2022 Bartłomiej Maryńczak. <https://github.com/PolyMeilex/rfd>
- **rmp 0.8.14** (declared `MIT`). Copyright (c) 2017 Evgeny Safronov. <https://github.com/3Hren/msgpack-rust>
- **rmp-serde 0.15.5** (declared `MIT`). Copyright (c) 2017 Evgeny Safronov. <https://github.com/3Hren/msgpack-rust>
- **rusqlite 0.32.1** (declared `MIT`). Copyright (c) 2014-2021 The rusqlite developers. <https://github.com/rusqlite/rusqlite>
- **rustc-demangle 0.1.26** (declared `MIT/Apache-2.0`). Copyright (c) 2014 Alex Crichton. <https://github.com/rust-lang/rustc-demangle>
- **rustc-hash 1.1.0** (declared `Apache-2.0/MIT`). No copyright line in the published crate; Cargo.toml authors: The Rust Project Developers. <https://github.com/rust-lang-nursery/rustc-hash>
- **rustc-hash 2.1.3** (declared `Apache-2.0 OR MIT`). No copyright line in the published crate; Cargo.toml authors: The Rust Project Developers. <https://github.com/rust-lang/rustc-hash>
- **rustls 0.23.35** (declared `Apache-2.0 OR ISC OR MIT`). Copyright (c) 2016, Joseph Birr-Pixton <jpixton@gmail.com>; Copyright (c) 2016 Joseph Birr-Pixton <jpixton@gmail.com>. <https://github.com/rustls/rustls>
- **rustls-native-certs 0.8.4** (declared `Apache-2.0 OR ISC OR MIT`). Copyright (c) 2016, Joseph Birr-Pixton <jpixton@gmail.com>; Copyright (c) 2016 Joseph Birr-Pixton <jpixton@gmail.com>. <https://github.com/rustls/rustls-native-certs>
- **rustls-pki-types 1.13.2** (declared `MIT OR Apache-2.0`). Copyright 2023 Dirkjan Ochtman; Copyright (c) 2023 Dirkjan Ochtman <dirkjan@ochtman.nl>. <https://github.com/rustls/pki-types>
- **rustversion 1.0.22** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: David Tolnay. <https://github.com/dtolnay/rustversion> [procedural macro: runs at build time, its expansion is compiled in]
- **scopeguard 1.2.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2016-2019 Ulrik Sverdrup "bluss" and scopeguard developers. <https://github.com/bluss/scopeguard>
- **seahash 4.1.0** (declared `MIT`). No copyright line in the published crate; Cargo.toml authors: ticki, Tom Almeida. <https://gitlab.redox-os.org/redox-os/seahash>
- **security-framework 3.7.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2015 Steven Fackler. <https://github.com/kornelski/rust-security-framework>
- **security-framework-sys 2.17.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2015 Steven Fackler. <https://github.com/kornelski/rust-security-framework>
- **serde 1.0.229** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: Erick Tryzelaar, David Tolnay. <https://github.com/serde-rs/serde>
- **serde_core 1.0.229** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: Erick Tryzelaar, David Tolnay. <https://github.com/serde-rs/serde>
- **serde_derive 1.0.229** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: Erick Tryzelaar, David Tolnay. <https://github.com/serde-rs/serde> [procedural macro: runs at build time, its expansion is compiled in]
- **serde_json 1.0.151** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: Erick Tryzelaar, David Tolnay. <https://github.com/serde-rs/json>
- **serde_urlencoded 0.7.1** (declared `MIT/Apache-2.0`). Copyright (c) 2016 Anthony Ramine. <https://github.com/nox/serde_urlencoded>
- **sha2 0.10.9** (declared `MIT OR Apache-2.0`). Copyright (c) 2006-2009 Graydon Hoare; Copyright (c) 2009-2013 Mozilla Foundation; Copyright (c) 2016 Artyom Pavlov. <https://github.com/RustCrypto/hashes>
- **sharded-slab 0.1.7** (declared `MIT`). Copyright (c) 2019 Eliza Weisman. <https://github.com/hawkw/sharded-slab>
- **simd-adler32 0.3.8** (declared `MIT`). Copyright (c) [2021] [Marvin Countryman]. <https://github.com/mcountryman/simd-adler32>
- **siphasher 1.0.1** (declared `MIT/Apache-2.0`). Copyright 2012-2016 The Rust Project Developers; Copyright 2016-2024 Frank Denis. <https://github.com/jedisct1/rust-siphash>
- **slab 0.4.11** (declared `MIT`). Copyright (c) 2019 Carl Lerche. <https://github.com/tokio-rs/slab>
- **small_btree 0.1.0** (declared `Unlicense OR MIT`). No copyright line in the published crate; Cargo.toml authors: boa-dev. <https://github.com/boa-dev/boa>
- **smallvec 1.15.1** (declared `MIT OR Apache-2.0`). Copyright (c) 2018 The Servo Project Developers. <https://github.com/servo/rust-smallvec>
- **socket2 0.6.1** (declared `MIT OR Apache-2.0`). Copyright (c) 2014 Alex Crichton. <https://github.com/rust-lang/socket2>
- **stable_deref_trait 1.2.1** (declared `MIT OR Apache-2.0`). Copyright (c) 2017 Robert Grosse. <https://github.com/storyyeller/stable_deref_trait>
- **static_assertions 1.1.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2017 Nikolai Vazquez. <https://github.com/nvzqz/static-assertions-rs>
- **strict-num 0.1.1** (declared `MIT`). Copyright (c) 2022 Yevhenii Reizner. <https://github.com/RazrFalcon/strict-num>
- **string_cache 0.8.9** (declared `MIT OR Apache-2.0`). Copyright (c) 2012-2013 Mozilla Foundation. <https://github.com/servo/string-cache>
- **strum 0.26.3** (declared `MIT`). Copyright (c) 2019 Peter Glotfelty. <https://github.com/Peternator7/strum>
- **strum 0.28.0** (declared `MIT`). Copyright (c) 2019 Peter Glotfelty. <https://github.com/Peternator7/strum>
- **strum_macros 0.26.4** (declared `MIT`). Copyright (c) 2019 Peter Glotfelty. <https://github.com/Peternator7/strum> [procedural macro: runs at build time, its expansion is compiled in]
- **strum_macros 0.28.0** (declared `MIT`). Copyright (c) 2019 Peter Glotfelty. <https://github.com/Peternator7/strum> [procedural macro: runs at build time, its expansion is compiled in]
- **syn 2.0.113** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: David Tolnay. <https://github.com/dtolnay/syn>
- **syn 3.0.6** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: David Tolnay. <https://github.com/dtolnay/syn>
- **synstructure 0.13.2** (declared `MIT`). Copyright 2016 Nika Layzell. <https://github.com/mystor/synstructure>
- **synstructure 0.14.0** (declared `MIT`). Copyright 2016 Nika Layzell. <https://github.com/mystor/synstructure>
- **tag_ptr 0.1.0** (declared `Unlicense OR MIT`). No copyright line in the published crate; Cargo.toml authors: boa-dev. <https://github.com/boa-dev/boa>
- **tap 1.0.1** (declared `MIT`). Copyright (c) 2017 Elliot Linder <darfink@gmail.com>. <https://github.com/myrrlyn/tap>
- **temporal_rs 0.2.6** (declared `MIT OR Apache-2.0`). Copyright 2024 Boa; Copyright (c) 2024 Boa. <https://github.com/boa-dev/temporal>
- **termcolor 1.4.1** (declared `Unlicense OR MIT`). Copyright (c) 2015 Andrew Gallant. <https://github.com/BurntSushi/termcolor>
- **thin-vec 0.2.20** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: Aria Beingessner. <https://github.com/mozilla/thin-vec>
- **thiserror 1.0.69** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: David Tolnay. <https://github.com/dtolnay/thiserror>
- **thiserror 2.0.21** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: David Tolnay. <https://github.com/dtolnay/thiserror>
- **thiserror-impl 1.0.69** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: David Tolnay. <https://github.com/dtolnay/thiserror> [procedural macro: runs at build time, its expansion is compiled in]
- **thiserror-impl 2.0.21** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: David Tolnay. <https://github.com/dtolnay/thiserror> [procedural macro: runs at build time, its expansion is compiled in]
- **thread_local 1.1.9** (declared `MIT OR Apache-2.0`). Copyright (c) 2016 The Rust Project Developers. <https://github.com/Amanieu/thread_local-rs>
- **time 0.3.55** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: Jacob Pratt, Time contributors. <https://github.com/time-rs/time>
- **time-core 0.1.9** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: Jacob Pratt, Time contributors. <https://github.com/time-rs/time>
- **time-macros 0.2.32** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: Jacob Pratt, Time contributors. <https://github.com/time-rs/time> [procedural macro: runs at build time, its expansion is compiled in]
- **timezone_provider 0.2.6** (declared `MIT OR Apache-2.0`). Copyright 2024 Boa; Copyright (c) 2024 Boa. <https://github.com/boa-dev/temporal>
- **tokio 1.49.0** (declared `MIT`). Copyright (c) Tokio Contributors. <https://github.com/tokio-rs/tokio>
- **tokio-macros 2.6.0** (declared `MIT`). Copyright (c) 2019 Yoshua Wuyts; Copyright (c) Tokio Contributors. <https://github.com/tokio-rs/tokio> [procedural macro: runs at build time, its expansion is compiled in]
- **tokio-rustls 0.26.4** (declared `MIT OR Apache-2.0`). Copyright 2017 quininer kel; Copyright (c) 2017 quininer kel. <https://github.com/rustls/tokio-rustls>
- **tokio-util 0.7.17** (declared `MIT`). Copyright (c) Tokio Contributors. <https://github.com/tokio-rs/tokio>
- **toml_datetime 0.7.5+spec-1.1.0** (declared `MIT OR Apache-2.0`). Copyright (c) Individual contributors. <https://github.com/toml-rs/toml>
- **toml_edit 0.23.10+spec-1.0.0** (declared `MIT OR Apache-2.0`). Copyright (c) Individual contributors. <https://github.com/toml-rs/toml>
- **toml_parser 1.0.6+spec-1.1.0** (declared `MIT OR Apache-2.0`). Copyright (c) Individual contributors. <https://github.com/toml-rs/toml>
- **tower 0.5.2** (declared `MIT`). Copyright (c) 2019 Tower Contributors. <https://github.com/tower-rs/tower>
- **tower-http 0.6.8** (declared `MIT`). Copyright (c) 2019-2021 Tower Contributors. <https://github.com/tower-rs/tower-http>
- **tower-layer 0.3.3** (declared `MIT`). Copyright (c) 2019 Tower Contributors. <https://github.com/tower-rs/tower>
- **tower-service 0.3.3** (declared `MIT`). Copyright (c) 2019 Tower Contributors. <https://github.com/tower-rs/tower>
- **tracing 0.1.44** (declared `MIT`). Copyright (c) 2019 Tokio Contributors. <https://github.com/tokio-rs/tracing>
- **tracing-attributes 0.1.31** (declared `MIT`). Copyright (c) 2019 Tokio Contributors. <https://github.com/tokio-rs/tracing> [procedural macro: runs at build time, its expansion is compiled in]
- **tracing-core 0.1.36** (declared `MIT`). Copyright (c) 2019 Tokio Contributors. <https://github.com/tokio-rs/tracing>
- **tracing-log 0.2.0** (declared `MIT`). Copyright (c) 2019 Tokio Contributors. <https://github.com/tokio-rs/tracing>
- **tracing-serde 0.2.0** (declared `MIT`). Copyright (c) 2019 Tokio Contributors. <https://github.com/tokio-rs/tracing>
- **tracing-subscriber 0.3.22** (declared `MIT`). Copyright (c) 2019 Tokio Contributors. <https://github.com/tokio-rs/tracing>
- **try-lock 0.2.5** (declared `MIT`). Copyright (c) 2018-2023 Sean McArthur; Copyright (c) 2016 Alex Crichton. <https://github.com/seanmonstar/try-lock>
- **typenum 1.19.0** (declared `MIT OR Apache-2.0`). Copyright 2014 Paho Lurie-Gregg; Copyright (c) 2014 Paho Lurie-Gregg. <https://github.com/paholg/typenum>
- **unicase 2.8.1** (declared `MIT OR Apache-2.0`). Copyright (c) 2014-2017 Sean McArthur. <https://github.com/seanmonstar/unicase>
- **unicode-bidi 0.3.18** (declared `MIT OR Apache-2.0`). Copyright (c) 2015 The Rust Project Developers. <https://github.com/servo/unicode-bidi>
- **unicode-segmentation 1.12.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2015 The Rust Project Developers. <https://github.com/unicode-rs/unicode-segmentation>
- **unicode-width 0.1.14** (declared `MIT OR Apache-2.0`). Copyright (c) 2015 The Rust Project Developers. <https://github.com/unicode-rs/unicode-width>
- **unicode-xid 0.2.6** (declared `MIT OR Apache-2.0`). Copyright (c) 2015 The Rust Project Developers. <https://github.com/unicode-rs/unicode-xid>
- **universal-hash 0.5.1** (declared `MIT OR Apache-2.0`). Copyright (c) 2019-2020 RustCrypto Developers. <https://github.com/RustCrypto/traits>
- **url 2.5.7** (declared `MIT OR Apache-2.0`). Copyright (c) 2013-2025 The rust-url developers. <https://github.com/servo/rust-url>
- **urlencoding 2.1.3** (declared `MIT`). © 2016 Bertram Truong; © 2021 Kornel Lesiński. <https://github.com/kornelski/rust_urlencoding>
- **utf16_iter 1.0.5** (declared `Apache-2.0 OR MIT`). No copyright line in the published crate; Cargo.toml authors: Henri Sivonen. <https://github.com/hsivonen/utf16_iter>
- **utf8_iter 1.0.4** (declared `Apache-2.0 OR MIT`). No copyright line in the published crate; Cargo.toml authors: Henri Sivonen. <https://github.com/hsivonen/utf8_iter>
- **want 0.3.1** (declared `MIT`). Copyright (c) 2018-2019 Sean McArthur. <https://github.com/seanmonstar/want>
- **weezl 0.1.12** (declared `MIT OR Apache-2.0`). Copyright (c) HeroicKatora 2020. <https://github.com/image-rs/weezl>
- **wgpu 23.0.1** (declared `MIT OR Apache-2.0`). Copyright (c) 2021 The gfx-rs developers. <https://github.com/gfx-rs/wgpu>
- **wgpu 24.0.5** (declared `MIT OR Apache-2.0`). Copyright (c) 2021 The gfx-rs developers. <https://github.com/gfx-rs/wgpu>
- **wgpu-core 23.0.1** (declared `MIT OR Apache-2.0`). Copyright (c) 2021 The gfx-rs developers. <https://github.com/gfx-rs/wgpu>
- **wgpu-core 24.0.5** (declared `MIT OR Apache-2.0`). Copyright (c) 2021 The gfx-rs developers. <https://github.com/gfx-rs/wgpu>
- **wgpu-hal 23.0.1** (declared `MIT OR Apache-2.0`). Copyright (c) 2021 The gfx-rs developers. <https://github.com/gfx-rs/wgpu>
- **wgpu-hal 24.0.4** (declared `MIT OR Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: gfx-rs developers. <https://github.com/gfx-rs/wgpu>
- **wgpu-types 23.0.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2021 The gfx-rs developers. <https://github.com/gfx-rs/wgpu>
- **wgpu-types 24.0.0** (declared `MIT OR Apache-2.0`). Copyright (c) 2021 The gfx-rs developers. <https://github.com/gfx-rs/wgpu>
- **winnow 0.7.14** (declared `MIT`). No copyright line and no authors field in the published crate. <https://github.com/winnow-rs/winnow>
- **write16 1.0.0** (declared `Apache-2.0 OR MIT`). No copyright line and no authors field in the published crate. <https://github.com/hsivonen/write16>
- **wry 0.49.0** (declared `Apache-2.0 OR MIT`). Copyright (c) 2020-2023 Ngo Iok Ui & Tauri Programme within The Commons Conservancy. <https://github.com/tauri-apps/wry>
- **xsum 0.1.6** (declared `MIT`). Copyright 2015, 2018, 2021, 2024 Radford M. Neal; Copyright 2025 Keita Nonaka. <https://github.com/Gumichocopengin8/xsum.rs>
- **zerocopy 0.8.31** (declared `BSD-2-Clause OR Apache-2.0 OR MIT`). Copyright 2023 The Fuchsia Authors; Copyright 2019 The Fuchsia Authors. <https://github.com/google/zerocopy>
- **zeroize 1.8.2** (declared `Apache-2.0 OR MIT`). Copyright (c) 2018-2021 The RustCrypto Project Developers. <https://github.com/RustCrypto/utils>
- **zmij 1.0.9** (declared `MIT`). No copyright line in the published crate; Cargo.toml authors: David Tolnay. <https://github.com/dtolnay/zmij>

#### Text of the MIT License

Each component's copyright line above stands in for the `Copyright (c) <year> <holder>` line of the licence.

````text
Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
````

### Apache License 2.0 (`Apache-2.0`): 8 crates

- **calendrical_calculations 0.2.4** (declared `Apache-2.0`). Copyright 2023 The Unicode Consortium. <https://github.com/unicode-org/icu4x>
- **codespan-reporting 0.11.1** (declared `Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: Brendan Zabarauskas. <https://github.com/brendanzab/codespan>
- **flatbuffers 25.12.19** (declared `Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: Robert Winslow, FlatBuffers Maintainers. <https://github.com/google/flatbuffers>
- **ryu 1.0.22** (declared `Apache-2.0 OR BSL-1.0`). No copyright line in the published crate; Cargo.toml authors: David Tolnay. <https://github.com/dtolnay/ryu>
- **ryu-js 1.0.3** (declared `Apache-2.0 OR BSL-1.0`). No copyright line in the published crate; Cargo.toml authors: David Tolnay, boa-dev. <https://github.com/boa-dev/ryu-js>
- **sync_wrapper 1.0.2** (declared `Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: Actyx AG. <https://github.com/Actyx/sync_wrapper>
- **tao 0.31.1** (declared `Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: Tauri Programme within The Commons Conservancy, The winit contributors. <https://github.com/tauri-apps/tao>
- **unicode-linebreak 0.1.5** (declared `Apache-2.0`). No copyright line in the published crate; Cargo.toml authors: Axel Forsman. <https://github.com/axelf4/unicode-linebreak>

#### Text of the Apache License 2.0

````text
                              Apache License
                        Version 2.0, January 2004
                     http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS
````

### BSD 3-Clause License (`BSD-3-Clause`): 5 crates

- **alloc-no-stdlib 3.0.0** (declared `BSD-3-Clause`). Copyright (c) 2016 Dropbox, Inc. <https://github.com/dropbox/rust-alloc-no-stdlib>
- **alloc-stdlib 0.3.0** (declared `BSD-3-Clause`). No copyright line in the published crate; Cargo.toml authors: Daniel Reiter Horn. <https://github.com/dropbox/rust-alloc-no-stdlib>
- **subtle 2.6.1** (declared `BSD-3-Clause`). Copyright (c) 2016-2017 Isis Agora Lovecruft, Henry de Valence. All rights reserved; Copyright (c) 2016-2024 Isis Agora Lovecruft. All rights reserved. <https://github.com/dalek-cryptography/subtle>
- **tiny-skia 0.11.4** (declared `BSD-3-Clause`). Copyright (c) 2011 Google Inc. All rights reserved; Copyright (c) 2020 Yevhenii Reizner All rights reserved. <https://github.com/RazrFalcon/tiny-skia>
- **tiny-skia-path 0.11.4** (declared `BSD-3-Clause`). Copyright (c) 2011 Google Inc. All rights reserved; Copyright (c) 2020 Yevhenii Reizner All rights reserved. <https://github.com/RazrFalcon/tiny-skia/tree/master/path>

#### Text of the BSD 3-Clause License

Each component's copyright line above stands in for the copyright line of the licence; "the copyright holder" in clause 3 is that component's holder (for tiny-skia: Google Inc. and Yevhenii Reizner).

````text
Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its contributors
   may be used to endorse or promote products derived from this software
   without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND
ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
````

### BSD 2-Clause License (`BSD-2-Clause`): 1 crate

- **arrayref 0.3.9** (declared `BSD-2-Clause`). Copyright (c) 2015 David Roundy <roundyd@physics.oregonstate.edu>. <https://github.com/droundy/arrayref>

#### Text of the BSD 2-Clause License

````text
Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

1. Redistributions of source code must retain the above copyright
   notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright
   notice, this list of conditions and the following disclaimer in the
   documentation and/or other materials provided with the
   distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
````

### ISC License (`ISC`): 3 crates

- **libloading 0.8.9** (declared `ISC`). Copyright © 2015, Simonas Kazlauskas. <https://github.com/nagisa/rust_libloading/>
- **rustls-webpki 0.103.8** (declared `ISC`). Copyright 2015 Brian Smith. <https://github.com/rustls/webpki>
- **untrusted 0.9.0** (declared `ISC`). Copyright 2015-2016 Brian Smith. <https://github.com/briansmith/untrusted>

#### Text of the ISC License

````text
Permission to use, copy, modify, and/or distribute this software for any purpose with or without
fee is hereby granted, provided that the above copyright notice and this permission notice appear
in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS
SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE
AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT,
NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF
THIS SOFTWARE.
````

### zlib License (`Zlib`): 2 crates

- **foldhash 0.1.5** (declared `Zlib`). Copyright (c) 2024 Orson Peters. <https://github.com/orlp/foldhash>
- **foldhash 0.2.0** (declared `Zlib`). Copyright (c) 2024 Orson Peters. <https://github.com/orlp/foldhash>

#### Text of the zlib License

````text
This software is provided 'as-is', without any express or implied warranty. In
no event will the authors be held liable for any damages arising from the use of
this software.

Permission is granted to anyone to use this software for any purpose, including
commercial applications, and to alter it and redistribute it freely, subject to
the following restrictions:

1. The origin of this software must not be misrepresented; you must not claim
    that you wrote the original software. If you use this software in a product,
    an acknowledgment in the product documentation would be appreciated but is
    not required.

2. Altered source versions must be plainly marked as such, and must not be
    misrepresented as being the original software.

3. This notice may not be removed or altered from any source distribution.
````

### Mozilla Public License 2.0 (`MPL-2.0`): 2 crates

- **adblock 0.12.0** (declared `MPL-2.0`). No copyright line in the published crate; Cargo.toml authors: Anton Lazarev, Andrius Aucinas. <https://github.com/brave/adblock-rust/>
- **option-ext 0.2.0** (declared `MPL-2.0`). No copyright line in the published crate; Cargo.toml authors: Simon Ochsenreither. <https://github.com/soc/option-ext.git>

#### Text of the Mozilla Public License 2.0

The same text as `LICENSE` at the repository root, which is HiWave's own licence.

````text
Mozilla Public License Version 2.0
==================================

1. Definitions
--------------

1.1. "Contributor"
    means each individual or legal entity that creates, contributes to
    the creation of, or owns Covered Software.

1.2. "Contributor Version"
    means the combination of the Contributions of others (if any) used
    by a Contributor and that particular Contributor's Contribution.

1.3. "Contribution"
    means Covered Software of a particular Contributor.

1.4. "Covered Software"
    means Source Code Form to which the initial Contributor has attached
    the notice in Exhibit A, the Executable Form of such Source Code
    Form, and Modifications of such Source Code Form, in each case
    including portions thereof.

1.5. "Incompatible With Secondary Licenses"
    means

    (a) that the initial Contributor has attached the notice described
        in Exhibit B to the Covered Software; or

    (b) that the Covered Software was made available under the terms of
        version 1.1 or earlier of the License, but not also under the
        terms of a Secondary License.

1.6. "Executable Form"
    means any form of the work other than Source Code Form.

1.7. "Larger Work"
    means a work that combines Covered Software with other material, in
    a separate file or files, that is not Covered Software.

1.8. "License"
    means this document.

1.9. "Licensable"
    means having the right to grant, to the maximum extent possible,
    whether at the time of the initial grant or subsequently, any and
    all of the rights conveyed by this License.

1.10. "Modifications"
    means any of the following:

    (a) any file in Source Code Form that results from an addition to,
        deletion from, or modification of the contents of Covered
        Software; or

    (b) any new file in Source Code Form that contains any Covered
        Software.

1.11. "Patent Claims" of a Contributor
    means any patent claim(s), including without limitation, method,
    process, and apparatus claims, in any patent Licensable by such
    Contributor that would be infringed, but for the grant of the
    License, by the making, using, selling, offering for sale, having
    made, import, or transfer of either its Contributions or its
    Contributor Version.

1.12. "Secondary License"
    means either the GNU General Public License, Version 2.0, the GNU
    Lesser General Public License, Version 2.1, the GNU Affero General
    Public License, Version 3.0, or any later versions of those
    licenses.

1.13. "Source Code Form"
    means the form of the work preferred for making modifications.

1.14. "You" (or "Your")
    means an individual or a legal entity exercising rights under this
    License. For legal entities, "You" includes any entity that
    controls, is controlled by, or is under common control with You. For
    purposes of this definition, "control" means (a) the power, direct
    or indirect, to cause the direction or management of such entity,
    whether by contract or otherwise, or (b) ownership of more than
    fifty percent (50%) of the outstanding shares or beneficial
    ownership of such entity.

2. License Grants and Conditions
--------------------------------

2.1. Grants

Each Contributor hereby grants You a world-wide, royalty-free,
non-exclusive license:

(a) under intellectual property rights (other than patent or trademark)
    Licensable by such Contributor to use, reproduce, make available,
    modify, display, perform, distribute, and otherwise exploit its
    Contributions, either on an unmodified basis, with Modifications, or
    as part of a Larger Work; and

(b) under Patent Claims of such Contributor to make, use, sell, offer
    for sale, have made, import, and otherwise transfer either its
    Contributions or its Contributor Version.

2.2. Effective Date

The licenses granted in Section 2.1 with respect to any Contribution
become effective for each Contribution on the date the Contributor first
distributes such Contribution.

2.3. Limitations on Grant Scope

The licenses granted in this Section 2 are the only rights granted under
this License. No additional rights or licenses will be implied from the
distribution or licensing of Covered Software under this License.
Notwithstanding Section 2.1(b) above, no patent license is granted by a
Contributor:

(a) for any code that a Contributor has removed from Covered Software;
    or

(b) for infringements caused by: (i) Your and any other third party's
    modifications of Covered Software, or (ii) the combination of its
    Contributions with other software (except as part of its Contributor
    Version); or

(c) under Patent Claims infringed by Covered Software in the absence of
    its Contributions.

This License does not grant any rights in the trademarks, service marks,
or logos of any Contributor (except as may be necessary to comply with
the notice requirements in Section 3.4).

2.4. Subsequent Licenses

No Contributor makes additional grants as a result of Your choice to
distribute the Covered Software under a subsequent version of this
License (see Section 10.2) or under the terms of a Secondary License (if
permitted under the terms of Section 3.3).

2.5. Representation

Each Contributor represents that the Contributor believes its
Contributions are its original creation(s) or it has sufficient rights
to grant the rights to its Contributions conveyed by this License.

2.6. Fair Use

This License is not intended to limit any rights You have under
applicable copyright doctrines of fair use, fair dealing, or other
equivalents.

2.7. Conditions

Sections 3.1, 3.2, 3.3, and 3.4 are conditions of the licenses granted
in Section 2.1.

3. Responsibilities
-------------------

3.1. Distribution of Source Form

All distribution of Covered Software in Source Code Form, including any
Modifications that You create or to which You contribute, must be under
the terms of this License. You must inform recipients that the Source
Code Form of the Covered Software is governed by the terms of this
License, and how they can obtain a copy of this License. You may not
attempt to alter or restrict the recipients' rights in the Source Code
Form.

3.2. Distribution of Executable Form

If You distribute Covered Software in Executable Form then:

(a) such Covered Software must also be made available in Source Code
    Form, as described in Section 3.1, and You must inform recipients of
    the Executable Form how they can obtain a copy of such Source Code
    Form by reasonable means in a timely manner, at a charge no more
    than the cost of distribution to the recipient; and

(b) You may distribute such Executable Form under the terms of this
    License, or sublicense it under different terms, provided that the
    license for the Executable Form does not attempt to limit or alter
    the recipients' rights in the Source Code Form under this License.

3.3. Distribution of a Larger Work

You may create and distribute a Larger Work under terms of Your choice,
provided that You also comply with the requirements of this License for
the Covered Software. If the Larger Work is a combination of Covered
Software with a work governed by one or more Secondary Licenses, and the
Covered Software is not Incompatible With Secondary Licenses, this
License permits You to additionally distribute such Covered Software
under the terms of such Secondary License(s), so that the recipient of
the Larger Work may, at their option, further distribute the Covered
Software under the terms of either this License or such Secondary
License(s).

3.4. Notices

You may not remove or alter the substance of any license notices
(including copyright notices, patent notices, disclaimers of warranty,
or limitations of liability) contained within the Source Code Form of
the Covered Software, except that You may alter any license notices to
the extent required to remedy known factual inaccuracies.

3.5. Application of Additional Terms

You may choose to offer, and to charge a fee for, warranty, support,
indemnity or liability obligations to one or more recipients of Covered
Software. However, You may do so only on Your own behalf, and not on
behalf of any Contributor. You must make it absolutely clear that any
such warranty, support, indemnity, or liability obligation is offered by
You alone, and You hereby agree to indemnify every Contributor for any
liability incurred by such Contributor as a result of warranty, support,
indemnity or liability terms You offer. You may include additional
disclaimers of warranty and limitations of liability specific to any
jurisdiction.

4. Inability to Comply Due to Statute or Regulation
---------------------------------------------------

If it is impossible for You to comply with any of the terms of this
License with respect to some or all of the Covered Software due to
statute, judicial order, or regulation then You must: (a) comply with
the terms of this License to the maximum extent possible; and (b)
describe the limitations and the code they affect. Such description must
be placed in a text file included with all distributions of the Covered
Software under this License. Except to the extent prohibited by statute
or regulation, such description must be sufficiently detailed for a
recipient of ordinary skill to be able to understand it.

5. Termination
--------------

5.1. The rights granted under this License will terminate automatically
if You fail to comply with any of its terms. However, if You become
compliant, then the rights granted under this License from a particular
Contributor are reinstated (a) provisionally, unless and until such
Contributor explicitly and finally terminates Your grants, and (b) on an
ongoing basis, if such Contributor fails to notify You of the
non-compliance by some reasonable means prior to 60 days after You have
come back into compliance. Moreover, Your grants from a particular
Contributor are reinstated on an ongoing basis if such Contributor
notifies You of the non-compliance by some reasonable means, this is the
first time You have received notice of non-compliance with this License
from such Contributor, and You become compliant prior to 30 days after
Your receipt of the notice.

5.2. If You initiate litigation against any entity by asserting a patent
infringement claim (excluding declaratory judgment actions,
counter-claims, and cross-claims) alleging that a Contributor Version
directly or indirectly infringes any patent, then the rights granted to
You by any and all Contributors for the Covered Software under Section
2.1 of this License shall terminate.

5.3. In the event of termination under Sections 5.1 or 5.2 above, all
end user license agreements (excluding distributors and resellers) which
have been validly granted by You or Your distributors under this License
prior to termination shall survive termination.

************************************************************************
*                                                                      *
*  6. Disclaimer of Warranty                                           *
*  -------------------------                                           *
*                                                                      *
*  Covered Software is provided under this License on an "as is"       *
*  basis, without warranty of any kind, either expressed, implied, or  *
*  statutory, including, without limitation, warranties that the       *
*  Covered Software is free of defects, merchantable, fit for a        *
*  particular purpose or non-infringing. The entire risk as to the     *
*  quality and performance of the Covered Software is with You.        *
*  Should any Covered Software prove defective in any respect, You     *
*  (not any Contributor) assume the cost of any necessary servicing,   *
*  repair, or correction. This disclaimer of warranty constitutes an   *
*  essential part of this License. No use of any Covered Software is   *
*  authorized under this License except under this disclaimer.         *
*                                                                      *
************************************************************************

************************************************************************
*                                                                      *
*  7. Limitation of Liability                                          *
*  --------------------------                                          *
*                                                                      *
*  Under no circumstances and under no legal theory, whether tort      *
*  (including negligence), contract, or otherwise, shall any           *
*  Contributor, or anyone who distributes Covered Software as          *
*  permitted above, be liable to You for any direct, indirect,         *
*  special, incidental, or consequential damages of any character      *
*  including, without limitation, damages for lost profits, loss of    *
*  goodwill, work stoppage, computer failure or malfunction, or any    *
*  and all other commercial damages or losses, even if such party      *
*  shall have been informed of the possibility of such damages. This   *
*  limitation of liability shall not apply to liability for death or   *
*  personal injury resulting from such party's negligence to the       *
*  extent applicable law prohibits such limitation. Some               *
*  jurisdictions do not allow the exclusion or limitation of           *
*  incidental or consequential damages, so this exclusion and          *
*  limitation may not apply to You.                                    *
*                                                                      *
************************************************************************

8. Litigation
-------------

Any litigation relating to this License may be brought only in the
courts of a jurisdiction where the defendant maintains its principal
place of business and such litigation shall be governed by laws of that
jurisdiction, without reference to its conflict-of-law provisions.
Nothing in this Section shall prevent a party's ability to bring
cross-claims or counter-claims.

9. Miscellaneous
----------------

This License represents the complete agreement concerning the subject
matter hereof. If any provision of this License is held to be
unenforceable, such provision shall be reformed only to the extent
necessary to make it enforceable. Any law or regulation which provides
that the language of a contract shall be construed against the drafter
shall not be used to construe this License against a Contributor.

10. Versions of the License
---------------------------

10.1. New Versions

Mozilla Foundation is the license steward. Except as provided in Section
10.3, no one other than the license steward has the right to modify or
publish new versions of this License. Each version will be given a
distinguishing version number.

10.2. Effect of New Versions

You may distribute the Covered Software under the terms of the version
of the License under which You originally received the Covered Software,
or under the terms of any subsequent version published by the license
steward.

10.3. Modified Versions

If you create software not governed by this License, and you want to
create a new license for such software, you may create and use a
modified version of this License if you rename the license and remove
any references to the name of the license steward (except to note that
such modified license differs from this License).

10.4. Distributing Source Code Form that is Incompatible With Secondary
Licenses

If You choose to distribute Source Code Form that is Incompatible With
Secondary Licenses under the terms of this version of the License, the
notice described in Exhibit B of this License must be attached.

Exhibit A - Source Code Form License Notice
-------------------------------------------

  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at http://mozilla.org/MPL/2.0/.

If it is not possible or desirable to put the notice in a particular
file, then You may include the notice in a location (such as a LICENSE
file in a relevant directory) where a recipient would be likely to look
for such a notice.

You may add additional accurate notices of copyright ownership.

Exhibit B - "Incompatible With Secondary Licenses" Notice
---------------------------------------------------------

  This Source Code Form is "Incompatible With Secondary Licenses", as
  defined by the Mozilla Public License, v. 2.0.
````

### Unicode License v3 (`Unicode-3.0`): 23 crates

- **icu_calendar 2.3.0** (declared `Unicode-3.0`). Copyright © 2020-2024 Unicode, Inc. <https://github.com/unicode-org/icu4x>
- **icu_calendar_data 2.3.0** (declared `Unicode-3.0`). Copyright © 2020-2024 Unicode, Inc. <https://github.com/unicode-org/icu4x>
- **icu_collections 2.3.0** (declared `Unicode-3.0`). Copyright © 2020-2024 Unicode, Inc. <https://github.com/unicode-org/icu4x>
- **icu_locale_core 2.3.0** (declared `Unicode-3.0`). Copyright © 2020-2024 Unicode, Inc. <https://github.com/unicode-org/icu4x>
- **icu_locale_fallback 2.3.0** (declared `Unicode-3.0`). Copyright © 2020-2024 Unicode, Inc. <https://github.com/unicode-org/icu4x>
- **icu_locale_fallback_data 2.3.0** (declared `Unicode-3.0`). Copyright © 2020-2024 Unicode, Inc. <https://github.com/unicode-org/icu4x>
- **icu_normalizer 2.3.0** (declared `Unicode-3.0`). Copyright © 2020-2024 Unicode, Inc. <https://github.com/unicode-org/icu4x>
- **icu_normalizer_data 2.3.0** (declared `Unicode-3.0`). Copyright © 2020-2024 Unicode, Inc. <https://github.com/unicode-org/icu4x>
- **icu_properties 2.3.0** (declared `Unicode-3.0`). Copyright © 2020-2024 Unicode, Inc. <https://github.com/unicode-org/icu4x>
- **icu_properties_data 2.3.0** (declared `Unicode-3.0`). Copyright © 2020-2024 Unicode, Inc. <https://github.com/unicode-org/icu4x>
- **icu_provider 2.3.1** (declared `Unicode-3.0`). Copyright © 2020-2024 Unicode, Inc. <https://github.com/unicode-org/icu4x>
- **ixdtf 0.6.6** (declared `Unicode-3.0`). Copyright © 2020-2024 Unicode, Inc. <https://github.com/unicode-org/icu4x>
- **litemap 0.8.1** (declared `Unicode-3.0`). Copyright © 2020-2024 Unicode, Inc. <https://github.com/unicode-org/icu4x>
- **potential_utf 0.1.4** (declared `Unicode-3.0`). Copyright © 2020-2024 Unicode, Inc. <https://github.com/unicode-org/icu4x>
- **tinystr 0.8.4** (declared `Unicode-3.0`). Copyright © 2020-2024 Unicode, Inc. <https://github.com/unicode-org/icu4x>
- **writeable 0.6.4** (declared `Unicode-3.0`). Copyright © 2020-2024 Unicode, Inc. <https://github.com/unicode-org/icu4x>
- **yoke 0.8.3** (declared `Unicode-3.0`). Copyright © 2020-2024 Unicode, Inc. <https://github.com/unicode-org/icu4x>
- **yoke-derive 0.8.4** (declared `Unicode-3.0`). Copyright © 2020-2024 Unicode, Inc. <https://github.com/unicode-org/icu4x> [procedural macro: runs at build time, its expansion is compiled in]
- **zerofrom 0.1.6** (declared `Unicode-3.0`). Copyright © 2020-2024 Unicode, Inc. <https://github.com/unicode-org/icu4x>
- **zerofrom-derive 0.1.6** (declared `Unicode-3.0`). Copyright © 2020-2024 Unicode, Inc. <https://github.com/unicode-org/icu4x> [procedural macro: runs at build time, its expansion is compiled in]
- **zerotrie 0.2.5** (declared `Unicode-3.0`). Copyright © 2020-2024 Unicode, Inc. <https://github.com/unicode-org/icu4x>
- **zerovec 0.11.8** (declared `Unicode-3.0`). Copyright © 2020-2024 Unicode, Inc. <https://github.com/unicode-org/icu4x>
- **zerovec-derive 0.11.6** (declared `Unicode-3.0`). Copyright © 2020-2024 Unicode, Inc. <https://github.com/unicode-org/icu4x> [procedural macro: runs at build time, its expansion is compiled in]

For the ICU4X crates above (repository `unicode-org/icu4x`) the licence file adds:

> Portions of ICU4X may have been adapted from ICU4C and/or ICU4J.
> ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.

#### Text of the Unicode License v3

````text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

[the copyright line of each component is in the list above]

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0
````

### Community Data License Agreement, Permissive, 2.0 (`CDLA-Permissive-2.0`): 1 crate

- **webpki-roots 1.0.5** (declared `CDLA-Permissive-2.0`). No copyright line and no authors field in the published crate. <https://github.com/rustls/webpki-roots>

#### Text of the Community Data License Agreement, Permissive, 2.0

````text
# Community Data License Agreement - Permissive - Version 2.0

This is the Community Data License Agreement - Permissive, Version
2.0 (the "agreement"). Data Provider(s) and Data Recipient(s) agree
as follows:

## 1. Provision of the Data

1.1. A Data Recipient may use, modify, and share the Data made
available by Data Provider(s) under this agreement if that Data
Recipient follows the terms of this agreement.

1.2. This agreement does not impose any restriction on a Data
Recipient's use, modification, or sharing of any portions of the
Data that are in the public domain or that may be used, modified,
or shared under any other legal exception or limitation.

## 2. Conditions for Sharing Data

2.1. A Data Recipient may share Data, with or without modifications, so
long as the Data Recipient makes available the text of this agreement
with the shared Data.

## 3. No Restrictions on Results

3.1. This agreement does not impose any restriction or obligations
with respect to the use, modification, or sharing of Results.

## 4. No Warranty; Limitation of Liability

4.1. All Data Recipients receive the Data subject to the following
terms:

THE DATA IS PROVIDED ON AN "AS IS" BASIS, WITHOUT REPRESENTATIONS,
WARRANTIES OR CONDITIONS OF ANY KIND, EITHER EXPRESS OR IMPLIED
INCLUDING, WITHOUT LIMITATION, ANY WARRANTIES OR CONDITIONS OF TITLE,
NON-INFRINGEMENT, MERCHANTABILITY OR FITNESS FOR A PARTICULAR PURPOSE.

NO DATA PROVIDER SHALL HAVE ANY LIABILITY FOR ANY DIRECT, INDIRECT,
INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING
WITHOUT LIMITATION LOST PROFITS), HOWEVER CAUSED AND ON ANY THEORY OF
LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE DATA OR RESULTS,
EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGES.

## 5. Definitions

5.1. "Data" means the material received by a Data Recipient under
this agreement.

5.2. "Data Provider" means any person who is the source of Data
provided under this agreement and in reliance on a Data Recipient's
agreement to its terms.

5.3. "Data Recipient" means any person who receives Data directly
or indirectly from a Data Provider and agrees to the terms of this
agreement.

5.4. "Results" means any outcome obtained by computational analysis
of Data, including for example machine learning models and models'
insights.
````

### Creative Commons CC0 1.0 Universal (`CC0-1.0`): 1 crate

- **hexf-parse 0.2.1** (declared `CC0-1.0`). No copyright line in the published crate; Cargo.toml authors: Kang Seonghoon. <https://github.com/lifthrasiir/hexf>

#### Text of the Creative Commons CC0 1.0 Universal

````text
Creative Commons Legal Code

CC0 1.0 Universal

    CREATIVE COMMONS CORPORATION IS NOT A LAW FIRM AND DOES NOT PROVIDE
    LEGAL SERVICES. DISTRIBUTION OF THIS DOCUMENT DOES NOT CREATE AN
    ATTORNEY-CLIENT RELATIONSHIP. CREATIVE COMMONS PROVIDES THIS
    INFORMATION ON AN "AS-IS" BASIS. CREATIVE COMMONS MAKES NO WARRANTIES
    REGARDING THE USE OF THIS DOCUMENT OR THE INFORMATION OR WORKS
    PROVIDED HEREUNDER, AND DISCLAIMS LIABILITY FOR DAMAGES RESULTING FROM
    THE USE OF THIS DOCUMENT OR THE INFORMATION OR WORKS PROVIDED
    HEREUNDER.

Statement of Purpose

The laws of most jurisdictions throughout the world automatically confer
exclusive Copyright and Related Rights (defined below) upon the creator
and subsequent owner(s) (each and all, an "owner") of an original work of
authorship and/or a database (each, a "Work").

Certain owners wish to permanently relinquish those rights to a Work for
the purpose of contributing to a commons of creative, cultural and
scientific works ("Commons") that the public can reliably and without fear
of later claims of infringement build upon, modify, incorporate in other
works, reuse and redistribute as freely as possible in any form whatsoever
and for any purposes, including without limitation commercial purposes.
These owners may contribute to the Commons to promote the ideal of a free
culture and the further production of creative, cultural and scientific
works, or to gain reputation or greater distribution for their Work in
part through the use and efforts of others.

For these and/or other purposes and motivations, and without any
expectation of additional consideration or compensation, the person
associating CC0 with a Work (the "Affirmer"), to the extent that he or she
is an owner of Copyright and Related Rights in the Work, voluntarily
elects to apply CC0 to the Work and publicly distribute the Work under its
terms, with knowledge of his or her Copyright and Related Rights in the
Work and the meaning and intended legal effect of CC0 on those rights.

1. Copyright and Related Rights. A Work made available under CC0 may be
protected by copyright and related or neighboring rights ("Copyright and
Related Rights"). Copyright and Related Rights include, but are not
limited to, the following:

  i. the right to reproduce, adapt, distribute, perform, display,
     communicate, and translate a Work;
 ii. moral rights retained by the original author(s) and/or performer(s);
iii. publicity and privacy rights pertaining to a person's image or
     likeness depicted in a Work;
 iv. rights protecting against unfair competition in regards to a Work,
     subject to the limitations in paragraph 4(a), below;
  v. rights protecting the extraction, dissemination, use and reuse of data
     in a Work;
 vi. database rights (such as those arising under Directive 96/9/EC of the
     European Parliament and of the Council of 11 March 1996 on the legal
     protection of databases, and under any national implementation
     thereof, including any amended or successor version of such
     directive); and
vii. other similar, equivalent or corresponding rights throughout the
     world based on applicable law or treaty, and any national
     implementations thereof.

2. Waiver. To the greatest extent permitted by, but not in contravention
of, applicable law, Affirmer hereby overtly, fully, permanently,
irrevocably and unconditionally waives, abandons, and surrenders all of
Affirmer's Copyright and Related Rights and associated claims and causes
of action, whether now known or unknown (including existing as well as
future claims and causes of action), in the Work (i) in all territories
worldwide, (ii) for the maximum duration provided by applicable law or
treaty (including future time extensions), (iii) in any current or future
medium and for any number of copies, and (iv) for any purpose whatsoever,
including without limitation commercial, advertising or promotional
purposes (the "Waiver"). Affirmer makes the Waiver for the benefit of each
member of the public at large and to the detriment of Affirmer's heirs and
successors, fully intending that such Waiver shall not be subject to
revocation, rescission, cancellation, termination, or any other legal or
equitable action to disrupt the quiet enjoyment of the Work by the public
as contemplated by Affirmer's express Statement of Purpose.

3. Public License Fallback. Should any part of the Waiver for any reason
be judged legally invalid or ineffective under applicable law, then the
Waiver shall be preserved to the maximum extent permitted taking into
account Affirmer's express Statement of Purpose. In addition, to the
extent the Waiver is so judged Affirmer hereby grants to each affected
person a royalty-free, non transferable, non sublicensable, non exclusive,
irrevocable and unconditional license to exercise Affirmer's Copyright and
Related Rights in the Work (i) in all territories worldwide, (ii) for the
maximum duration provided by applicable law or treaty (including future
time extensions), (iii) in any current or future medium and for any number
of copies, and (iv) for any purpose whatsoever, including without
limitation commercial, advertising or promotional purposes (the
"License"). The License shall be deemed effective as of the date CC0 was
applied by Affirmer to the Work. Should any part of the License for any
reason be judged legally invalid or ineffective under applicable law, such
partial invalidity or ineffectiveness shall not invalidate the remainder
of the License, and in such case Affirmer hereby affirms that he or she
will not (i) exercise any of his or her remaining Copyright and Related
Rights in the Work or (ii) assert any associated claims and causes of
action with respect to the Work, in either case contrary to Affirmer's
express Statement of Purpose.

4. Limitations and Disclaimers.

 a. No trademark or patent rights held by Affirmer are waived, abandoned,
    surrendered, licensed or otherwise affected by this document.
 b. Affirmer offers the Work as-is and makes no representations or
    warranties of any kind concerning the Work, express, implied,
    statutory or otherwise, including without limitation warranties of
    title, merchantability, fitness for a particular purpose, non
    infringement, or the absence of latent or other defects, accuracy, or
    the present or absence of errors, whether or not discoverable, all to
    the greatest extent permissible under applicable law.
 c. Affirmer disclaims responsibility for clearing rights of other persons
    that may apply to the Work or any use thereof, including without
    limitation any person's Copyright and Related Rights in the Work.
    Further, Affirmer disclaims responsibility for obtaining any necessary
    consents, permissions or other rights required for any use of the
    Work.
 d. Affirmer understands and acknowledges that Creative Commons is not a
    party to this document and has no duty or obligation with respect to
    this CC0 or use of the Work.
````

## 5. Crates under more than one licence at once, and licence files in full

These declare an `AND` expression: every licence named applies to some part of
the crate.

- **aws-lc-rs 1.18.1** (declared `ISC AND (Apache-2.0 OR ISC)`). Copyright Amazon.com, Inc. or its affiliates, and the other holders named in its licence files, which are reproduced in full in section 5. <https://github.com/aws/aws-lc-rs>
- **aws-lc-sys 0.45.0** (declared `ISC AND (Apache-2.0 OR ISC) AND Apache-2.0 AND MIT AND BSD-3-Clause AND (Apache-2.0 OR ISC OR MIT) AND (Apache-2.0 OR ISC OR MIT-0)`). Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved, and the other holders named in its licence files, which are reproduced in full in section 5. <https://github.com/aws/aws-lc-rs>
- **dpi 0.1.2** (declared `Apache-2.0 AND MIT`). Copyright (c) 2018 Jorge Aparicio, and the other holders named in its licence files, which are reproduced in full in section 5. <https://github.com/rust-windowing/winit>
- **ring 0.17.14** (declared `Apache-2.0 AND ISC`). Copyright (c) 2009 The Go Authors. All rights reserved, and the other holders named in its licence files, which are reproduced in full in section 5. <https://github.com/briansmith/ring>
- **unicode-ident 1.0.22** (declared `(MIT OR Apache-2.0) AND Unicode-3.0`). Copyright © 1991-2023 Unicode, Inc. <https://github.com/dtolnay/unicode-ident>

- **dpi**: Apache-2.0 for the crate (text in section 4) and MIT for the code it
  takes from libm; its `LICENSE-LIBM-MIT` follows in full.
- **unicode-ident**: used under MIT for its code and Unicode-3.0 for the Unicode
  Character Database tables it embeds (Copyright (c) 1991-2023 Unicode, Inc.);
  both texts are in section 4.
- **aws-lc-rs**, **aws-lc-sys** and **ring**: their licence files follow in full,
  because they attribute individual parts (AWS-LC, BoringSSL, OpenSSL, SSLeay
  and others) that a summary would lose.
- **libm** (listed under MIT in section 4) is one licence, but its `LICENSE.txt`
  names several holders for parts ported from musl and FreeBSD; it follows in
  full for the same reason.

#### dpi 0.1.2: `LICENSE-LIBM-MIT`

````text
rust-lang/libm as a whole is available for use under the MIT license:

------------------------------------------------------------------------------
Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
------------------------------------------------------------------------------

This Rust library contains the following copyrights:

    Copyright (c) 2018 Jorge Aparicio

Portions of this software are derived from third-party works licensed under
terms compatible with the above MIT license:

* musl libc https://www.musl-libc.org/. This library contains the following
  copyright:

      Copyright © 2005-2020 Rich Felker, et al.

* The CORE-MATH project https://core-math.gitlabpages.inria.fr/. CORE-MATH
  routines are available under the MIT license on a per-file basis.

The musl libc COPYRIGHT file also includes the following notice relevant to
math portions of the library:

------------------------------------------------------------------------------
Much of the math library code (src/math/* and src/complex/*) is
Copyright © 1993,2004 Sun Microsystems or
Copyright © 2003-2011 David Schultz or
Copyright © 2003-2009 Steven G. Kargl or
Copyright © 2003-2009 Bruce D. Evans or
Copyright © 2008 Stephen L. Moshier or
Copyright © 2017-2018 Arm Limited
and labelled as such in comments in the individual source files. All
have been licensed under extremely permissive terms.
------------------------------------------------------------------------------
````

#### libm 0.2.16: `LICENSE.txt`

````text
rust-lang/libm as a whole is available for use under the MIT license:

------------------------------------------------------------------------------
Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
------------------------------------------------------------------------------

As a contributor, you agree that your code can be used under either the MIT
license or the Apache-2.0 license:

------------------------------------------------------------------------------
                                 Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS

   APPENDIX: How to apply the Apache License to your work.

      To apply the Apache License to your work, attach the following
      boilerplate notice, with the fields enclosed by brackets "[]"
      replaced with your own identifying information. (Don't include
      the brackets!)  The text should be enclosed in the appropriate
      comment syntax for the file format. We also recommend that a
      file or class name and description of purpose be included on the
      same "printed page" as the copyright notice for easier
      identification within third-party archives.

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
------------------------------------------------------------------------------

This Rust library contains the following copyrights:

    Copyright (c) 2018 Jorge Aparicio

Portions of this software are derived from third-party works licensed under
terms compatible with the above MIT license:

* musl libc https://www.musl-libc.org/. This library contains the following
  copyright:

      Copyright © 2005-2020 Rich Felker, et al.

* The CORE-MATH project https://core-math.gitlabpages.inria.fr/. CORE-MATH
  routines are available under the MIT license on a per-file basis.

The musl libc COPYRIGHT file also includes the following notice relevant to
math portions of the library:

------------------------------------------------------------------------------
Much of the math library code (src/math/* and src/complex/*) is
Copyright © 1993,2004 Sun Microsystems or
Copyright © 2003-2011 David Schultz or
Copyright © 2003-2009 Steven G. Kargl or
Copyright © 2003-2009 Bruce D. Evans or
Copyright © 2008 Stephen L. Moshier or
Copyright © 2017-2018 Arm Limited
and labelled as such in comments in the individual source files. All
have been licensed under extremely permissive terms.
------------------------------------------------------------------------------

Copyright notices are retained in src/* files where relevant.
````

#### aws-lc-rs 1.18.1: `LICENSE`

````text
SPDX-License-Identifier: ISC AND (Apache-2.0 OR ISC)


Apache 2.0 license
-------------------------------------


                                 Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS


ISC license
-------------------------------------


Copyright Amazon.com, Inc. or its affiliates.

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
````

#### aws-lc-sys 0.45.0: `LICENSE`

````text
AWS Libcrypto (AWS-LC)

AWS-LC is a fork of BoringSSL, which is itself a fork of OpenSSL.
Content from these and other sources retains their original licensing,
as described below. New files from AWS-LC are made available under the Apache-2.0 license OR the ISC
license. These licenses are reproduced at the bottom of this file.

```
Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
SPDX-License-Identifier: Apache-2.0 OR ISC
```


================================================================================
BoringSSL
================================================================================

BoringSSL is a Google-maintained fork of OpenSSL. Historically, code
authored by Google for BoringSSL was licensed under the ISC License.
BoringSSL has since relicensed upstream to Apache License 2.0. Existing
AWS-LC code originating from BoringSSL retains its ISC license, while
newer code taken from BoringSSL is licensed under Apache License 2.0.

```
Copyright (c) 2014-2024 Google Inc.
SPDX-License-Identifier: ISC
```

Additional individual contributions to BoringSSL-derived code are
covered under the ISC license:

  - Brian Smith (Copyright 2016)
  - Robert Nagy (Copyright 2022)
  - Arm Ltd (Copyright 2020)

```
Copyright (c) 2025-2026 Google Inc.
SPDX-License-Identifier: Apache-2.0
```

================================================================================
OpenSSL
================================================================================

Code derived from the OpenSSL project is licensed under the
Apache License, Version 2.0.

```
Copyright (c) 1998-2011 The OpenSSL Project. All rights reserved.
SPDX-License-Identifier: Apache-2.0
```

Some OpenSSL-derived files also carry the original SSLeay copyright:

```
Copyright (c) 1995-1998 Eric Young (eay@cryptsoft.com). All rights reserved.
SPDX-License-Identifier: Apache-2.0
```

Portions of OpenSSL-derived code include contributions from:

  - Sun Microsystems, Inc. (Copyright 2002)
  - Nokia (Copyright 2005)
  - Intel Corporation (Copyright 2012-2021)

These contributions are covered under the Apache-2.0 license.

================================================================================
mlkem-native
================================================================================

Code from the mlkem-native project is licensed under Apache License 2.0 or MIT or ISC.

```
Copyright (c) The mlkem-native project authors.
SPDX-License-Identifier: Apache-2.0 OR ISC OR MIT
```

================================================================================
mldsa-native
================================================================================

Code from the mldsa-native project is licensed under Apache License 2.0 or MIT or ISC

```
Copyright (c) The mldsa-native project authors.
SPDX-License-Identifier: Apache-2.0 OR ISC OR MIT
```

================================================================================
Third-Party Libraries (compiled into libcrypto/libssl)
================================================================================

Fiat Cryptography
-----------------
Synthesizing Correct-by-Construction Code for Cryptographic Primitives.
See third_party/fiat/LICENSE.

```
Copyright (c) 2015-2020 the fiat-crypto authors.
SPDX-License-Identifier: MIT
```

s2n-bignum
----------
Integer arithmetic routines for cryptography.
See third_party/s2n-bignum/s2n-bignum-imported/LICENSE.

```
Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
SPDX-License-Identifier: Apache-2.0 OR ISC OR MIT-0
```

Note: ML-KEM/SHA3 code within s2n-bignum is licensed as
Apache-2.0 OR ISC OR MIT (with attribution), sourced from the
mlkem-native project.

Jitter Entropy RNG
-------------------
CPU Jitter Random Number Generator Library.
See third_party/jitterentropy/jitterentropy-library/LICENSE.

The Jitter Entropy library is dual-licensed under a BSD-style license
and the GNU General Public License Version 2. Amazon expressly elects
to distribute the package under the 3-Clause BSD License and NOT under
GNU General Public License Version 2.

```
Copyright (C) 2017 - 2025, Stephan Mueller <smueller@chronox.de>.
SPDX-License-Identifier: BSD-3-Clause
```

Keccak / AES Reference Implementations
---------------------------------------
Public domain (CC0) contributions.
https://creativecommons.org/public-domain/cc0

Code from sources and by authors listed in comments on top of the respective files.


================================================================================
Third-Party Libraries (NOT compiled into libcrypto/libssl)
================================================================================

The following are used for testing and build tooling only. Distributing
code linked against AWS-LC (libcrypto/libssl) does NOT trigger these
license obligations.

Google Test
-----------
See third_party/googletest/LICENSE.

```
Copyright 2008 Google Inc.
SPDX-License-Identifier: BSD-3-Clause
```

Go Standard Library
-------------------
Code in ssl/test/runner/ is derived from the Go standard library.

```
Copyright (c) The Go Authors. All rights reserved.
SPDX-License-Identifier: BSD-3-Clause
```

Wycheproof Test Vectors
------------------------
Project Wycheproof is a community managed repository of test vectors that can be used by cryptography library
developers to test against known attacks, specification inconsistencies, and other various implementation bugs.

See third_party/wycheproof_testvectors/LICENSE.

```
SPDX-License-Identifier: Apache-2.0
```


================================================================================
Full License Texts
================================================================================

Apache License 2.0
------------------
Apache License
Version 2.0, January 2004
http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

    1. Definitions.

        "License" shall mean the terms and conditions for use, reproduction, and distribution as defined by Sections 1 through 9 of this document.

        "Licensor" shall mean the copyright owner or entity authorized by the copyright owner that is granting the License.

        "Legal Entity" shall mean the union of the acting entity and all other entities that control, are controlled by, or are under common control with that entity. For the purposes of this definition, "control" means (i) the power, direct or indirect, to cause the direction or management of such entity, whether by contract or otherwise, or (ii) ownership of fifty percent (50%) or more of the outstanding shares, or (iii) beneficial ownership of such entity.

        "You" (or "Your") shall mean an individual or Legal Entity exercising permissions granted by this License.

        "Source" form shall mean the preferred form for making modifications, including but not limited to software source code, documentation source, and configuration files.

        "Object" form shall mean any form resulting from mechanical transformation or translation of a Source form, including but not limited to compiled object code, generated documentation, and conversions to other media types.

        "Work" shall mean the work of authorship, whether in Source or Object form, made available under the License, as indicated by a copyright notice that is included in or attached to the work (an example is provided in the Appendix below).

        "Derivative Works" shall mean any work, whether in Source or Object form, that is based on (or derived from) the Work and for which the editorial revisions, annotations, elaborations, or other modifications represent, as a whole, an original work of authorship. For the purposes of this License, Derivative Works shall not include works that remain separable from, or merely link (or bind by name) to the interfaces of, the Work and Derivative Works thereof.

        "Contribution" shall mean any work of authorship, including the original version of the Work and any modifications or additions to that Work or Derivative Works thereof, that is intentionally submitted to Licensor for inclusion in the Work by the copyright owner or by an individual or Legal Entity authorized to submit on behalf of the copyright owner. For the purposes of this definition, "submitted" means any form of electronic, verbal, or written communication sent to the Licensor or its representatives, including but not limited to communication on electronic mailing lists, source code control systems, and issue tracking systems that are managed by, or on behalf of, the Licensor for the purpose of discussing and improving the Work, but excluding communication that is conspicuously marked or otherwise designated in writing by the copyright owner as "Not a Contribution."

        "Contributor" shall mean Licensor and any individual or Legal Entity on behalf of whom a Contribution has been received by Licensor and subsequently incorporated within the Work.
    2. Grant of Copyright License. Subject to the terms and conditions of this License, each Contributor hereby grants to You a perpetual, worldwide, non-exclusive, no-charge, royalty-free, irrevocable copyright license to reproduce, prepare Derivative Works of, publicly display, publicly perform, sublicense, and distribute the Work and such Derivative Works in Source or Object form.
    3. Grant of Patent License. Subject to the terms and conditions of this License, each Contributor hereby grants to You a perpetual, worldwide, non-exclusive, no-charge, royalty-free, irrevocable (except as stated in this section) patent license to make, have made, use, offer to sell, sell, import, and otherwise transfer the Work, where such license applies only to those patent claims licensable by such Contributor that are necessarily infringed by their Contribution(s) alone or by combination of their Contribution(s) with the Work to which such Contribution(s) was submitted. If You institute patent litigation against any entity (including a cross-claim or counterclaim in a lawsuit) alleging that the Work or a Contribution incorporated within the Work constitutes direct or contributory patent infringement, then any patent licenses granted to You under this License for that Work shall terminate as of the date such litigation is filed.
    4. Redistribution. You may reproduce and distribute copies of the Work or Derivative Works thereof in any medium, with or without modifications, and in Source or Object form, provided that You meet the following conditions:
        (a) You must give any other recipients of the Work or Derivative Works a copy of this License; and
        (b) You must cause any modified files to carry prominent notices stating that You changed the files; and
        (c) You must retain, in the Source form of any Derivative Works that You distribute, all copyright, patent, trademark, and attribution notices from the Source form of the Work, excluding those notices that do not pertain to any part of the Derivative Works; and
        (d) If the Work includes a "NOTICE" text file as part of its distribution, then any Derivative Works that You distribute must include a readable copy of the attribution notices contained within such NOTICE file, excluding those notices that do not pertain to any part of the Derivative Works, in at least one of the following places: within a NOTICE text file distributed as part of the Derivative Works; within the Source form or documentation, if provided along with the Derivative Works; or, within a display generated by the Derivative Works, if and wherever such third-party notices normally appear. The contents of the NOTICE file are for informational purposes only and do not modify the License. You may add Your own attribution notices within Derivative Works that You distribute, alongside or as an addendum to the NOTICE text from the Work, provided that such additional attribution notices cannot be construed as modifying the License.

    You may add Your own copyright statement to Your modifications and may provide additional or different license terms and conditions for use, reproduction, or distribution of Your modifications, or for any such Derivative Works as a whole, provided Your use, reproduction, and distribution of the Work otherwise complies with the conditions stated in this License.
    5. Submission of Contributions. Unless You explicitly state otherwise, any Contribution intentionally submitted for inclusion in the Work by You to the Licensor shall be under the terms and conditions of this License, without any additional terms or conditions. Notwithstanding the above, nothing herein shall supersede or modify the terms of any separate license agreement you may have executed with Licensor regarding such Contributions.
    6. Trademarks. This License does not grant permission to use the trade names, trademarks, service marks, or product names of the Licensor, except as required for reasonable and customary use in describing the origin of the Work and reproducing the content of the NOTICE file.
    7. Disclaimer of Warranty. Unless required by applicable law or agreed to in writing, Licensor provides the Work (and each Contributor provides its Contributions) on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied, including, without limitation, any warranties or conditions of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A PARTICULAR PURPOSE. You are solely responsible for determining the appropriateness of using or redistributing the Work and assume any risks associated with Your exercise of permissions under this License.
    8. Limitation of Liability. In no event and under no legal theory, whether in tort (including negligence), contract, or otherwise, unless required by applicable law (such as deliberate and grossly negligent acts) or agreed to in writing, shall any Contributor be liable to You for damages, including any direct, indirect, special, incidental, or consequential damages of any character arising as a result of this License or out of the use or inability to use the Work (including but not limited to damages for loss of goodwill, work stoppage, computer failure or malfunction, or any and all other commercial damages or losses), even if such Contributor has been advised of the possibility of such damages.
    9. Accepting Warranty or Additional Liability. While redistributing the Work or Derivative Works thereof, You may choose to offer, and charge a fee for, acceptance of support, warranty, indemnity, or other liability obligations and/or rights consistent with this License. However, in accepting such obligations, You may act only on Your own behalf and on Your sole responsibility, not on behalf of any other Contributor, and only if You agree to indemnify, defend, and hold each Contributor harmless for any liability incurred by, or claims asserted against, such Contributor by reason of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

To apply the Apache License to your work, attach the following boilerplate notice, with the fields enclosed by brackets "[]" replaced with your own identifying information. (Don't include the brackets!) The text should be enclosed in the appropriate comment syntax for the file format. We also recommend that a file or class name and description of purpose be included on the same "printed page" as the copyright notice for easier identification within third-party archives.

Copyright [yyyy] [name of copyright owner]

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.

ISC License
-----------
Permission to use, copy, modify, and/or distribute this software for
any purpose with or without fee is hereby granted, provided that the
above copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL
WARRANTIES WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED
WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE
AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL
DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR
PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER
TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR
PERFORMANCE OF THIS SOFTWARE.

MIT License
-----------
Permission is hereby granted, free of charge, to any person obtaining
a copy of this software and associated documentation files (the
"Software"), to deal in the Software without restriction, including
without limitation the rights to use, copy, modify, merge, publish,
distribute, sublicense, and/or sell copies of the Software, and to
permit persons to whom the Software is furnished to do so, subject to
the following conditions:

The above copyright notice and this permission notice shall be
included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

BSD 3-Clause License
--------------------
Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions
are met:

1. Redistributions of source code must retain the above copyright
   notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright
   notice, this list of conditions and the following disclaimer in the
   documentation and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
   contributors may be used to endorse or promote products derived
   from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

MIT No Attribution (MIT-0)
--------------------------
Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
````

#### ring 0.17.14: `LICENSE`

````text
*ring* uses an "ISC" license, like BoringSSL used to use, for new code
files. See LICENSE-other-bits for the text of that license.

See LICENSE-BoringSSL for code that was sourced from BoringSSL under the
Apache 2.0 license. Some code that was sourced from BoringSSL under the ISC
license. In each case, the license info is at the top of the file.

See src/polyfill/once_cell/LICENSE-APACHE and src/polyfill/once_cell/LICENSE-MIT
for the license to code that was sourced from the once_cell project.
````

#### ring 0.17.14: `LICENSE-other-bits`

````text
Copyright 2015-2025 Brian Smith.

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY
SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION
OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN
CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
````

#### ring 0.17.14: `LICENSE-BoringSSL`

````text

                                 Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS

   APPENDIX: How to apply the Apache License to your work.

      To apply the Apache License to your work, attach the following
      boilerplate notice, with the fields enclosed by brackets "[]"
      replaced with your own identifying information. (Don't include
      the brackets!)  The text should be enclosed in the appropriate
      comment syntax for the file format. We also recommend that a
      file or class name and description of purpose be included on the
      same "printed page" as the copyright notice for easier
      identification within third-party archives.

   Copyright [yyyy] [name of copyright owner]

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.


Licenses for support code
-------------------------

Parts of the TLS test suite are under the Go license. This code is not included
in BoringSSL (i.e. libcrypto and libssl) when compiled, however, so
distributing code linked against BoringSSL does not trigger this license:

Copyright (c) 2009 The Go Authors. All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

   * Redistributions of source code must retain the above copyright
notice, this list of conditions and the following disclaimer.
   * Redistributions in binary form must reproduce the above
copyright notice, this list of conditions and the following disclaimer
in the documentation and/or other materials provided with the
distribution.
   * Neither the name of Google Inc. nor the names of its
contributors may be used to endorse or promote products derived from
this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.


BoringSSL uses the Chromium test infrastructure to run a continuous build,
trybots etc. The scripts which manage this, and the script for generating build
metadata, are under the Chromium license. Distributing code linked against
BoringSSL does not trigger this license.

Copyright 2015 The Chromium Authors. All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

   * Redistributions of source code must retain the above copyright
notice, this list of conditions and the following disclaimer.
   * Redistributions in binary form must reproduce the above
copyright notice, this list of conditions and the following disclaimer
in the documentation and/or other materials provided with the
distribution.
   * Neither the name of Google Inc. nor the names of its
contributors may be used to endorse or promote products derived from
this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
````

#### ring 0.17.14: `src/polyfill/once_cell/LICENSE-MIT`

(ring's copy of code from the once_cell project, `MIT OR Apache-2.0`; used under MIT.)

````text
Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHOR OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
````

## 6. In the repository, not in the shipped binary

Listed so the next inventory does not have to rediscover them. None of these
is compiled into `hiwave` or copied into an app bundle.

- **Ahem test font** (`crates/rustkit-text/tests/fixtures/Ahem.{ttf,woff,woff2}`),
  from web-platform-tests; used by tests only.
- **Noto Sans** (`baselines/common/fonts/NotoSans-*.ttf`, SIL Open Font License
  1.1); used by the parity baselines only. HiWave bundles no fonts: text is
  drawn with the fonts installed on the system.
- **Test pages and images** under `fixtures/`, `websuite/`, `static-web-suite/`,
  `cases/`, `parity-*`, `goldens/` and `baselines/`, including the MDN
  learning-area gallery image a `rustkit-image` unit test embeds
  (`websuite/js-ladder/01-mdn/32-image-gallery/images/pic4.jpg`, inside
  `#[cfg(test)]`).
- **Node packages** under `tools/parity_oracle/node_modules` (Playwright,
  pixelmatch, pngjs): developer tooling.
- **Build-time and test-time crates**: `[build-dependencies]` and
  `[dev-dependencies]` across the workspace, and the crates only
  `hiwave-mcp`, `hiwave-smoke`, `parity-capture`, `rustkit-bench` and
  `rustkit-test` use.
- **Filter lists** (EasyList, EasyPrivacy): not bundled. A running HiWave
  downloads them from `easylist.to` into the user's cache
  (`crates/hiwave-shield/src/filter_lists.rs`).
- **System frameworks** (WebKit, AppKit, CoreText, Metal and the rest): part of
  macOS, linked dynamically, not redistributed.

## 7. How this file was made, and what was not verified

The crate list is the output of:

```bash
for t in aarch64-apple-darwin x86_64-apple-darwin; do
  cargo tree -p hiwave-app -e normal --prefix none --format '{p}|{l}|{r}' --target $t --locked
done | sed 's/ (\*)$//' | sort -u
```

with the lines whose path is under `crates/` (HiWave's own) removed. Licence
expressions and repository URLs are the `license` and `repository` fields of
each crate's `Cargo.toml`, as that command prints them. Copyright lines were
collected from the `LICENSE*`, `COPYING*`, `COPYRIGHT*`, `NOTICE*` and
`UNLICENSE*` files at the root of each crate's published source
(`~/.cargo/registry/src`). The non-crate works in section 2 were found by
listing every `include_str!` and `include_bytes!` in the workspace and by
searching the source for attribution comments.

To check that this file still covers the dependency set (prints every crate
and version that is missing; prints nothing when the file is complete):

```bash
for t in aarch64-apple-darwin x86_64-apple-darwin; do
  cargo tree -p hiwave-app -e normal --prefix none --format '{p}' --target $t --locked
done | sed 's/ (\*)$//' | grep -v '/crates/' | sort -u | awk '{print $1" "substr($2,2)}' |
  while read -r c; do grep -qF "**$c**" THIRD_PARTY_NOTICES.md || echo "missing: $c"; done
```

Not verified:

- Copyright lines are what each crate's own licence files say (292 crates).
  86 crates ship no copyright line; their entries give the `authors` field instead,
  which names authors, not necessarily copyright holders. Their upstream
  repositories were not read, except Boa's.
- Licence expressions are the crates' own declarations. No crate's source was
  audited for files under a licence other than the declared one.
- Licence files below a crate's root were not searched, except ring's
  `once_cell` copy, which its `LICENSE` names. ring's `LICENSE` also points at
  an Apache-2.0 copy for that code; the Apache-2.0 text is in section 4.
- Which licence to use where a crate offers a choice is a convention chosen for
  this file (stated in section 4), not something a lawyer reviewed.
- The list is for the macOS binary. The Windows and Linux ports resolve other
  platform crates and need their own files.
- HiWave has no packaging step in this repository; whatever builds the
  downloadable app must copy this file into it.
