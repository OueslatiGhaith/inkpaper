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
- [ ] text alignment (book default/left/justified)
- [ ] paragraph indent
- [ ] paragraph spacing
- [ ] option to override the book's CSS with these settings
- [ ] full Text Settings screen (Font | Size | Layout | Style tabs with a live preview)

## Hyphenation: if it doesn't cost too much

- [ ] hyphenation dictionaries
- [ ] break words at hyphenation points during line layout
- [ ] hyphenation setting (on/off)
- [ ] pick the dictionary from the book's language

## Links

- [ ] internal links between chapters and anchors
- [ ] footnotes
- [ ] return to the previous position after following a link

## Home

- [ ] extract and cache the current book's cover
- [ ] show the cover on the home screen instead of the placeholder

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
