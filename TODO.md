# TODO List

# Next release: reader experience

New UI follows crosspoint/crossink's screens and layout.

## Fonts

- [x] separate the UI font from the reader font
- [x] keep Inter for the UI
- [x] use Libron for the reader
- [x] bold reader face
- [x] italic reader faces
- [x] find font families in `/.fonts` and `/fonts` like crosspoint, and list them in the Font tab
- [x] custom reader fonts loaded from storage (TTF/OTF, a collection's first face, up to 4 MB a family)
- [x] font picker in the reader settings
- [ ] derive bold and italic for card families without them, like crosspoint
- [ ] stream fonts too large to read whole, such as CJK ones

## Typography settings

- [x] line spacing
- [x] page margins
- [x] persist typography settings
- [x] repaginate after any typography change
- [x] Text panel in the reader drawer, with crosspoint's option picker
- [x] paragraph indent
- [x] paragraph spacing
- [x] Embedded Style switch for the book's own CSS
- [x] paragraph alignment (justify/left/center/right/book's style)
- [x] justified lines, stretching the spaces between words
- [x] full Text Settings screen (Font | Size | Layout | Style tabs with a live preview)
- [ ] word spacing and character spacing, crosspoint's other Layout rows

## Hyphenation

- [x] hyphenation dictionaries (English)
- [x] break words at hyphenation points during line layout
- [x] hyphenation setting (on/off)
- [x] pick the dictionary from the book's language
- [ ] more languages: crosspoint's French, German, Russian, Spanish, Italian, Polish, Swedish, Ukrainian, Finnish and Portuguese
- [ ] draw a hyphen where a line breaks at a book's own soft hyphen

## Links

- [x] underline internal links
- [x] follow internal links by tapping them, between chapters and to anchors
- [x] return to the previous position after following a link (Home, up to 3 back)
- [x] footnotes: "Links and footnotes" in the drawer's More tab

## Home

- [x] find the cover a book declares (EPUB 3 cover-image, EPUB 2 meta, guide)
- [x] extract and cache the current book's cover
- [x] show the cover on the home screen instead of the placeholder

## Dictionary

- [x] read StarDict dictionaries: `.ifo`, `.idx`, `.dict` or dictzip `.dict.dz`, and `.syn` synonyms
- [x] find dictionaries in `/.dictionaries` and `/dictionaries`, one a folder, like crosspoint
- [x] sample each index once and save the samples, so lookups read only a few entries
- [x] Dictionary setting to pick one, or none
- [x] long press a word to select it and look it up, joining words broken across lines
- [x] look up the word's plain forms too (lowercase, English endings)
- [x] definition card with its own pages, turned with the side buttons
- [x] show the indexing during a dictionary's first lookup
- [ ] test on the device with a real Wiktionary dictionary: indexing time, lookup time, samples size
- [ ] keep a definition's HTML formatting (bold, italics, lists)
- [ ] select a phrase by dragging after the long press
- [ ] `.idx.gz` indexes and 64-bit offsets, which have to be unpacked or converted for now
- [ ] word forms for languages other than English

# Done

- reader: EPUB 2/3 open, render, CSS, images, pagination, chapter navigation, font size, status bar, saved progress
- home and library: home screen, continue reading, file browser, recent books, reading history
- frontlight: brightness, warmth, control center, persisted state
- battery and clock: battery status, charging state, RTC, clock/date
- power: long press deep sleep, sleep screen, auto sleep, fast wakeup
- display: short press power button to force refresh
- book transfer: USB mass storage

# Future goals

- [ ] daylight saving time rules for the timezone
