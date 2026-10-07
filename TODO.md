# TODO List

# Next release: reader experience

New UI follows crosspoint/crossink's screens and layout.

## Fonts

- [x] separate the UI font from the reader font
- [x] keep Inter for the UI
- [x] use Libron for the reader
- [x] bold reader face
- [x] italic reader faces
- [ ] custom reader fonts loaded from storage
- [ ] font picker in the reader settings

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
- [ ] footnotes: "Links and footnotes" in the drawer's More tab

## Home

- [x] find the cover a book declares (EPUB 3 cover-image, EPUB 2 meta, guide)
- [x] extract and cache the current book's cover
- [x] show the cover on the home screen instead of the placeholder

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
