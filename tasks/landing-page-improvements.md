# Landing Page Improvements

## Completed

### Wand Icon & Layout Redesign
- [x] Moved logo to compact header bar (28px on white rounded background)
- [x] Logo now inline with "Sorcery" title
- [x] Wand visible on white background

### Above-the-Fold Optimization
- [x] Reduced container padding (4rem → 1.5rem)
- [x] Tightened all spacing
- [x] Download button immediately visible with secondary links below
- [x] Features in compact 3-column grid
- [x] Curl and extension combined into 2-column "install methods" section
- [x] Shorter, punchier subtitle

---

## Remaining: Documentation

Decision: **mdBook** (Rust-native, simple markdown-to-HTML)

### Tasks
- [ ] Set up mdBook in sorcery-desktop repo
- [ ] Create documentation structure:
  - Overview / Getting Started
  - Editor setup (generate from constants)
  - Terminal click-to-open configuration
  - srcuri:// protocol specification
  - Sharing links (public vs private usage)
  - Browser extension usage
  - Security overview
- [ ] Configure build to generate editor list from code
- [ ] Add docs deployment to CI
