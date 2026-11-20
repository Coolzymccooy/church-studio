# Third-party notices

## LAME (MP3 export)

The desktop app encodes MP3 with **LAME 3.100** (<https://lame.sourceforge.io/>), built from
source and linked statically into the app through the Rust crates `mp3lame-sys` 0.1.11 and
`mp3lame-encoder` 0.2.5 (<https://github.com/DoumanAsh/mp3lame-encoder>).

- LAME is licensed under the GNU Library General Public License, version 2 or later:
  [licenses/LAME-LGPL-2.0.txt](licenses/LAME-LGPL-2.0.txt).
- The two Rust binding crates are licensed under the GNU LGPL, version 3:
  [licenses/mp3lame-rust-bindings-LGPL-3.0.txt](licenses/mp3lame-rust-bindings-LGPL-3.0.txt).

What the LGPL asks of us, and how it is met:

1. **Say that the library is used and under which licence.** This file, plus the licence texts
   above. Release builds should carry this notice and both texts (installer or About box).
2. **Let users replace the library.** The whole app is open source (MIT) at
   <https://github.com/Coolzymccooy/church-studio>, so anyone can rebuild it against a modified
   LAME. The LAME source used is the one bundled in the `mp3lame-sys` 0.1.11 crate on crates.io.
3. **Do not restrict reverse engineering for debugging such changes.** The MIT licence does not.

The MP3 patents expired in 2017; no patent licence is needed.

## AAC (M4A export)

AAC is encoded by the operating system's own encoder (AudioToolbox on macOS, Media Foundation on
Windows). Nothing is bundled and no licence text is needed for it.
