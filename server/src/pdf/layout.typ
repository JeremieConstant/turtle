// The layout of every PDF Export. It reads everything it shows from
// /data.json, which Turtle writes: the look, the words, and the content.
// Content is only ever used as strings, so nothing in a Page runs as Typst.

#let data = json("/data.json")
#let look = data.look

#let ink = rgb(look.text_color)
#let ink-strong = rgb(look.heading_color)
#let accent = rgb(look.accent_color)
#let soft = rgb("#4b4f59")
#let muted = rgb("#8b909a")
#let faint = rgb("#b9bcc3")
#let rule = rgb("#e6e7ea")
#let rule-soft = rgb("#f0f1f3")
#let wash = rgb("#f7f7f8")
#let tint(color) = color.lighten(93%)
#let callout-colors = (
  note: accent,
  tip: rgb("#2f855a"),
  warning: rgb("#b7791f"),
  danger: rgb("#c53030"),
)

// The PDF Style's logo as large as fits in the size given, in proportion.
#let logo(max-width, max-height) = context {
  let natural = measure(image(data.logo))
  if natural.height == 0pt or natural.width / natural.height > max-width / max-height {
    image(data.logo, width: max-width)
  } else {
    image(data.logo, height: max-height)
  }
}

#set document(title: data.title)
#set text(
  font: look.body_font,
  size: look.body_size * 1pt,
  fill: ink,
  lang: look.language,
  hyphenate: true,
  features: ("cv11", "ss01"),
)
#set par(leading: 0.78em, spacing: 3.4mm)
#set strong(delta: 200)
#show strong: set text(fill: ink-strong)
#show raw: set text(font: look.code_font)
// Headings are styled where they are made; they are headings for the PDF's
// bookmarks and the table of contents' page numbers.
#show heading: it => it.body

#set page(
  paper: "a4",
  margin: (top: 26mm, right: 22mm, bottom: 24mm, left: 22mm),
  header-ascent: 6mm,
  footer-descent: 7mm,
  header: align(bottom, text(size: 7.5pt, weight: 500, tracking: 0.06em, fill: faint, upper(data.title))),
  footer: {
    set text(size: 7.5pt, weight: 500, fill: faint)
    if data.logo != none {
      box(baseline: 2mm, logo(40mm, 6mm))
      h(3mm)
    }
    if look.footer_text != none { look.footer_text }
    h(1fr)
    text(fill: muted, context counter(page).display())
  },
)

// ---- Content

#let all(items, show-one) = {
  let joined = items.map(show-one).join()
  if joined == none { [] } else { joined }
}

#let fitted-image(path, alt) = box(layout(size => {
  let natural = image(path, alt: alt)
  if measure(natural).width > size.width { image(path, alt: alt, width: size.width) } else { natural }
}))

#let checkbox(checked) = {
  let color = if checked { accent } else { faint }
  box(
    width: 0.8em,
    height: 0.8em,
    baseline: 0.1em,
    radius: 1.5pt,
    stroke: 0.6pt + color,
    fill: if checked { color } else { none },
    if checked {
      place(curve(
        stroke: (paint: white, thickness: 1pt, cap: "round", join: "round"),
        curve.move((0.18em, 0.42em)),
        curve.line((0.34em, 0.58em)),
        curve.line((0.62em, 0.22em)),
      ))
    },
  )
}

// A directive Turtle does not know, shown as a grey placeholder naming it.
#let unknown(name) = text(size: 0.86em, fill: muted, data.labels.unknown_element + ": " + name)

#let inline-of(x) = {
  if x.type == "break" {
    linebreak()
  } else if x.type == "unknown" {
    box(fill: wash, stroke: 0.5pt + rule, radius: 3pt, inset: (x: 3pt), outset: (y: 2pt), unknown(x.name))
  } else if x.type == "image" {
    fitted-image(x.path, x.alt)
  } else if x.type == "mention" {
    box(
      fill: wash,
      stroke: 0.5pt + rule,
      radius: 0.7em,
      inset: (x: 0.45em),
      outset: (y: 2pt),
      text(weight: 500, fill: ink-strong, [#x.name]),
    )
  } else {
    let shown = if x.code {
      box(
        fill: wash,
        stroke: 0.5pt + rule,
        radius: 3pt,
        inset: (x: 3pt),
        outset: (y: 2pt),
        text(size: 0.86em, fill: ink-strong, raw(x.text)),
      )
    } else { [#x.text] }
    if x.bold { shown = strong(shown) }
    if x.italic { shown = emph(shown) }
    if x.strike { shown = strike(shown) }
    if x.link != none {
      let target = if "part" in x.link { label(x.link.part) } else { x.link.url }
      shown = link(target, text(fill: accent, underline(stroke: 0.6pt + accent.transparentize(72%), offset: 2pt, shown)))
    }
    shown
  }
}

#let inlines(xs) = all(xs, inline-of)

// A heading other content can link to, by its label. It keeps the size of
// the text around it, which Typst would make larger by its level: the PDF
// Style's sizes are the sizes headings have.
#let labelled-heading(level, title, name) = context {
  let shown = heading(level: level, outlined: false, text(size: text.size, title))
  if name == none { shown } else [#shown#label(name)]
}

#let block-of(b, level) = {
  if b.type == "paragraph" {
    par(inlines(b.inlines))
  } else if b.type == "heading" {
    let body = inlines(b.inlines)
    let shown = labelled-heading(level + b.rank - 1, body, b.label)
    if b.rank == 2 {
      block(above: 10mm, below: 3.5mm, sticky: true, text(font: look.heading_font, size: look.h2_size * 1pt, weight: 600, tracking: -0.02em, fill: ink-strong, shown))
    } else if b.rank == 3 {
      block(above: 7mm, below: 2.5mm, sticky: true, text(font: look.heading_font, size: look.h3_size * 1pt, weight: 600, tracking: -0.01em, fill: ink-strong, shown))
    } else {
      block(above: 6mm, below: 2mm, sticky: true, text(font: look.heading_font, size: 8pt, weight: 700, tracking: 0.08em, fill: soft, upper(shown)))
    }
  } else if b.type == "code" {
    block(
      width: 100%,
      fill: wash,
      stroke: 0.5pt + rule,
      radius: 5pt,
      inset: (x: 5mm, y: 4mm),
      below: 4mm,
      text(size: 8pt, raw(b.text, lang: b.language, block: true)),
    )
  } else if b.type == "quote" {
    block(width: 100%, above: 5mm, below: 5mm, inset: (left: 5mm, y: 1mm), stroke: (left: 2pt + rule), text(fill: soft, all(b.blocks, c => block-of(c, level))))
  } else if b.type == "callout" {
    let color = callout-colors.at(b.kind)
    block(
      width: 100%,
      above: 5mm,
      below: 5mm,
      inset: (x: 5mm, y: 3.8mm),
      fill: tint(color),
      stroke: (left: 2pt + color),
      radius: (right: 4pt),
      {
        block(below: 2mm, text(size: 7pt, weight: 600, tracking: 0.1em, fill: color, upper(data.labels.at(b.kind))))
        all(b.blocks, c => block-of(c, level))
      },
    )
  } else if b.type == "bullets" {
    list(
      marker: text(fill: faint)[–],
      body-indent: 3mm,
      spacing: 1.4mm,
      ..b.items.map(item => all(item, c => block-of(c, level))),
    )
  } else if b.type == "numbered" {
    enum(
      start: b.start,
      numbering: n => text(fill: accent, weight: 600, str(n) + "."),
      body-indent: 3mm,
      spacing: 1.4mm,
      ..b.items.map(item => all(item, c => block-of(c, level))),
    )
  } else if b.type == "tasks" {
    stack(
      spacing: 1.4mm,
      ..b.items.map(task => grid(
        columns: (auto, 1fr),
        column-gutter: 2.5mm,
        checkbox(task.checked), all(task.blocks, c => block-of(c, level)),
      )),
    )
  } else if b.type == "table" {
    let header = b.rows.first().map(cell => text(size: 6.8pt, weight: 700, tracking: 0.09em, fill: muted, upper(inlines(cell))))
    let body = b.rows.slice(1).map(row => row.enumerate().map(((index, cell)) => {
      if index == 0 { text(weight: 500, fill: ink-strong, inlines(cell)) } else { inlines(cell) }
    }))
    block(above: 4mm, below: 6mm, text(size: 8.4pt, table(
      columns: header.len(),
      align: left + top,
      inset: (left: 0pt, right: 3.5mm, y: 2.6mm),
      stroke: (x, y) => (bottom: if y == 0 { 0.8pt + ink-strong } else { 0.5pt + rule }),
      table.header(..header),
      ..body.flatten(),
    )))
  } else if b.type == "rule" {
    block(above: 8mm, below: 8mm, line(length: 100%, stroke: 0.6pt + rule))
  } else if b.type == "unknown" {
    block(width: 100%, above: 4mm, below: 4mm, fill: wash, stroke: 0.5pt + rule, radius: 4pt, inset: (x: 5mm, y: 3mm), unknown(b.name))
  }
}

// ---- Cover

#if look.cover {
  page(margin: (top: 26mm, x: 24mm, bottom: 24mm), header: none, footer: none, {
    if data.logo != none { logo(40mm, 20mm) }
    v(1fr)
    block(width: 140mm, {
      box(width: 22mm, height: 2.2pt, fill: accent, radius: 2pt)
      v(9mm)
      block(text(font: look.heading_font, size: 40pt, weight: 600, tracking: -0.035em, fill: ink-strong, hyphenate: false, par(leading: 0.3em, data.title)))
      if data.subtitle != none {
        v(6mm)
        text(size: 13pt, tracking: -0.01em, fill: soft, data.subtitle)
      }
    })
    v(22mm)
    line(length: 100%, stroke: 0.6pt + rule)
    v(6mm)
    text(size: 7pt, weight: 600, tracking: 0.1em, fill: muted, upper(data.labels.as_of))
    linebreak()
    text(size: 9.5pt, weight: 500, data.date)
  })
}

// ---- Table of contents

#let page-of(name) = context counter(page).at(label(name)).first()

#let contents-line(name, body, number-size) = link(label(name), {
  body
  box(width: 1fr, inset: (x: 2mm), repeat(gap: 2pt, text(fill: faint)[.]))
  text(size: number-size, fill: muted, page-of(name))
})

#if look.toc {
  block(below: 12mm, text(font: look.heading_font, size: look.h1_size * 1pt, weight: 600, tracking: -0.03em, fill: ink-strong, data.labels.contents))
  for (index, part) in data.parts.enumerate() {
    let indent = part.depth * 9mm
    if part.kind == "folder" {
      block(
        above: if index == 0 { 0mm } else { 9mm },
        below: 3mm,
        inset: (left: indent),
        sticky: true,
        link(label(part.label), text(size: 7pt, weight: 600, tracking: 0.12em, fill: muted, upper(part.title))),
      )
    } else {
      block(width: 100%, breakable: false, inset: (left: indent, bottom: 3.2mm), below: 3.2mm, stroke: (bottom: 0.5pt + rule-soft), {
        contents-line(part.label, text(size: 11pt, weight: 500, tracking: -0.01em, fill: ink-strong, part.title), 9pt)
        for section in part.sections.filter(s => s.rank <= look.toc_depth) {
          block(above: 1.8mm, inset: (left: 5mm * (section.rank - 1)), contents-line(section.label, text(size: 8.8pt, fill: soft, section.title), 8.5pt))
        }
      })
    }
  }
}

// ---- Chapters

#let after-folder = false
#for part in data.parts {
  // A Folder's heading starts the page of what is beneath it.
  if not after-folder { pagebreak(weak: true) }
  let level = part.depth + 1
  if part.kind == "folder" {
    block(below: 6mm, sticky: true, text(size: 8pt, weight: 600, tracking: 0.12em, fill: muted, upper(labelled-heading(level, part.title, part.label))))
  } else {
    block(below: 9mm, text(font: look.heading_font, size: look.h1_size * 1pt, weight: 600, tracking: -0.03em, fill: ink-strong, labelled-heading(level, part.title, part.label)))
    all(part.blocks, b => block-of(b, level))
  }
  after-folder = part.kind == "folder"
}
