# Third-Party Notices

AgentMux source code is licensed under the MIT License. Third-party components
retain their upstream licenses. This file records the non-source assets checked
into this repository and the dependency-license policy used for releases.

This notice is not a relicensing grant. Font files and dependencies listed here
are not covered by the AgentMux MIT License.

## Bundled Fonts

The desktop app bundles font assets so the Windows UI and terminal render
offline without relying on a CDN or system-installed programming fonts.

- Pretendard Variable
  - Source: https://github.com/orioncactus/pretendard
  - License: SIL Open Font License 1.1
  - Upstream license file: https://github.com/orioncactus/pretendard/blob/main/LICENSE
  - Bundled file: `apps/desktop/src/assets/fonts/PretendardVariable.woff2`

- D2Coding Nerd Font
  - Based on NAVER D2Coding with Nerd Fonts glyph patching.
  - D2Coding license: SIL Open Font License 1.1
  - D2Coding license reference: https://github.com/naver/d2codingfont/wiki/Open-Font-License
  - Nerd Fonts license reference: https://github.com/ryanoasis/nerd-fonts/blob/master/LICENSE
  - Bundled files:
    - `apps/desktop/src/assets/fonts/D2CodingNerd.woff`
    - `apps/desktop/src/assets/fonts/D2CodingNerd.woff2`

- Symbols Nerd Font Mono
  - Source: https://github.com/ryanoasis/nerd-fonts
  - License reference: https://github.com/ryanoasis/nerd-fonts/blob/master/LICENSE
  - Glyph source and license index: https://github.com/ryanoasis/nerd-fonts/tree/master/src/glyphs
  - Bundled file:
    - `apps/desktop/src/assets/fonts/SymbolsNerdFontMono-Regular.ttf`

## Dependency Licenses

Runtime and build dependencies are not vendored into the repository. Their
versions and declared licenses are recorded in lockfiles:

- Rust dependencies: `Cargo.lock`
- Desktop npm dependencies: `apps/desktop/package-lock.json`

Release license review should reject unknown, proprietary, or strong-copyleft
dependency licenses unless the maintainer has explicitly approved the dependency
and updated this notice. Current metadata checks showed no unknown external npm
dependency licenses and no unknown Cargo dependency licenses.

Known transitive dependency license families include MIT, Apache-2.0, BSD, ISC,
Zlib, Unicode-3.0, MPL-2.0, and CDLA-Permissive-2.0. Dependencies that offer
multiple license choices are consumed under a permissive option where available.

When updating dependencies or bundled assets, update this notice if a new asset
is checked into the repository or if a license obligation changes.
