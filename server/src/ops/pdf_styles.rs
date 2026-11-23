//! PDF Styles: named looks for PDF Exports, shared across the Workspace.
//! Workspace Admins manage them; whoever exports picks one or gets the default.

use super::accounts::{random_hex, Nothing};
use super::{areas, Catalogue, Core, OpError, OpResult, Principal};
use crate::pdf::{Language, Look};
use rusqlite::{Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue
        .add(
            "list_pdf_styles",
            "Lists every PDF Style with its values, marking the default one, and the font families a PDF Style may use. For anyone signed in, to choose one when exporting a PDF.",
            list_pdf_styles,
        )
        .changes = false;
    catalogue.add(
        "create_pdf_style",
        "Creates a PDF Style. Only for a Workspace Admin.",
        create_pdf_style,
    );
    catalogue.add(
        "update_pdf_style",
        "Changes every value of a PDF Style. Only for a Workspace Admin.",
        update_pdf_style,
    );
    catalogue.add(
        "set_default_pdf_style",
        "Makes a PDF Style the default, with which a PDF is made when none is chosen. Only for a Workspace Admin.",
        set_default_pdf_style,
    );
    catalogue.add(
        "delete_pdf_style",
        "Deletes a PDF Style. The default one cannot be deleted; make another one the default first. Only for a Workspace Admin.",
        delete_pdf_style,
    );
}

/// What a PDF Style is called and how a PDF made with it looks.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct PdfStyleValues {
    /// Unique in the Workspace.
    pub name: String,
    /// The colour of body text, as `#rrggbb`.
    pub text_color: String,
    /// The colour of headings, as `#rrggbb`.
    pub heading_color: String,
    /// The colour of links, rules, Callout bars and the table of contents' numbers, as `#rrggbb`.
    pub accent_color: String,
    /// The font family of body text, one of those `list_pdf_styles` names.
    pub body_font: String,
    /// The font family of headings and the cover's title.
    pub heading_font: String,
    /// The font family of code.
    pub code_font: String,
    /// The size of body text in points, from 8 to 14.
    pub body_size: f64,
    /// The size of a chapter's title in points, from 10 to 36.
    pub h1_size: f64,
    /// The size of a Page's top headings in points, from 10 to 36.
    pub h2_size: f64,
    /// The size of the headings below those in points, from 10 to 36.
    pub h3_size: f64,
    /// Whether the PDF begins with a cover.
    pub cover: bool,
    /// Whether the PDF has a table of contents.
    pub toc: bool,
    /// What the table of contents lists: 1 the chapters, 2 also their `##` Sections, 3 also `###`.
    pub toc_depth: u8,
    /// One line at the foot of every page but the cover, of at most 200 characters.
    #[serde(default)]
    pub footer_text: Option<String>,
    /// The language of the words the PDF shows itself, such as "Contents", and of its date.
    pub language: Language,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct PdfStyle {
    pub id: String,
    /// Whether a PDF is made with this PDF Style when none is chosen. Exactly one is.
    pub is_default: bool,
    #[serde(flatten)]
    pub values: PdfStyleValues,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ChangedPdfStyle {
    pub id: String,
    #[serde(flatten)]
    pub values: PdfStyleValues,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PdfStyleId {
    pub id: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct PdfStyleList {
    /// In order of name.
    pub pdf_styles: Vec<PdfStyle>,
    /// The font families a PDF Style may use.
    pub font_families: Vec<String>,
}

const COLUMNS: &str = "id, is_default, name, text_color, heading_color, accent_color, body_font, heading_font, \
     code_font, body_size, h1_size, h2_size, h3_size, cover, toc, toc_depth, footer_text, language";

pub fn list_pdf_styles(core: &Core, principal: &Principal, _: Nothing) -> OpResult<PdfStyleList> {
    areas::signed_in(principal)?;
    let db = core.db();
    let mut statement = db.prepare(&format!("SELECT {COLUMNS} FROM pdf_styles ORDER BY name COLLATE NOCASE, id"))?;
    let pdf_styles = statement.query_map([], read_style)?.collect::<Result<_, _>>()?;
    let font_families = font_families(&db)?;
    Ok(PdfStyleList { pdf_styles, font_families }.into())
}

pub fn create_pdf_style(core: &Core, principal: &Principal, input: PdfStyleValues) -> OpResult<PdfStyle> {
    require_workspace_admin(principal)?;
    let db = core.db();
    let values = valid(&db, input, None)?;
    let style = PdfStyle { id: random_hex(16)?, is_default: false, values };
    db.execute(
        "INSERT INTO pdf_styles (id, name, text_color, heading_color, accent_color, body_font, heading_font, code_font,
             body_size, h1_size, h2_size, h3_size, cover, toc, toc_depth, footer_text, language, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        rusqlite::params![
            style.id,
            style.values.name,
            style.values.text_color,
            style.values.heading_color,
            style.values.accent_color,
            style.values.body_font,
            style.values.heading_font,
            style.values.code_font,
            style.values.body_size,
            style.values.h1_size,
            style.values.h2_size,
            style.values.h3_size,
            style.values.cover,
            style.values.toc,
            style.values.toc_depth,
            style.values.footer_text,
            style.values.language.code(),
            core.now(),
        ],
    )?;
    Ok(style.into())
}

pub fn update_pdf_style(core: &Core, principal: &Principal, input: ChangedPdfStyle) -> OpResult<PdfStyle> {
    require_workspace_admin(principal)?;
    let db = core.db();
    let style = pdf_style(&db, Some(&input.id))?;
    let values = valid(&db, input.values, Some(&style.id))?;
    db.execute(
        "UPDATE pdf_styles SET name = ?2, text_color = ?3, heading_color = ?4, accent_color = ?5, body_font = ?6,
             heading_font = ?7, code_font = ?8, body_size = ?9, h1_size = ?10, h2_size = ?11, h3_size = ?12,
             cover = ?13, toc = ?14, toc_depth = ?15, footer_text = ?16, language = ?17
         WHERE id = ?1",
        rusqlite::params![
            style.id,
            values.name,
            values.text_color,
            values.heading_color,
            values.accent_color,
            values.body_font,
            values.heading_font,
            values.code_font,
            values.body_size,
            values.h1_size,
            values.h2_size,
            values.h3_size,
            values.cover,
            values.toc,
            values.toc_depth,
            values.footer_text,
            values.language.code(),
        ],
    )?;
    Ok(PdfStyle { values, ..style }.into())
}

pub fn set_default_pdf_style(core: &Core, principal: &Principal, input: PdfStyleId) -> OpResult<PdfStyle> {
    require_workspace_admin(principal)?;
    let mut db = core.db();
    let style = pdf_style(&db, Some(&input.id))?;
    let transaction = db.transaction()?;
    transaction.execute("UPDATE pdf_styles SET is_default = 0 WHERE is_default", [])?;
    transaction.execute("UPDATE pdf_styles SET is_default = 1 WHERE id = ?1", [&style.id])?;
    transaction.commit()?;
    Ok(PdfStyle { is_default: true, ..style }.into())
}

pub fn delete_pdf_style(core: &Core, principal: &Principal, input: PdfStyleId) -> OpResult<()> {
    require_workspace_admin(principal)?;
    let db = core.db();
    let style = pdf_style(&db, Some(&input.id))?;
    if style.is_default {
        return Err(OpError::Conflict("This is the default PDF Style. Make another one the default first.".into()));
    }
    db.execute("DELETE FROM pdf_styles WHERE id = ?1", [&style.id])?;
    Ok(().into())
}

/// The PDF Style with this ID, or the default one without an ID.
pub(super) fn pdf_style(db: &Connection, id: Option<&str>) -> Result<PdfStyle, OpError> {
    let found = match id {
        Some(id) => db.query_row(&format!("SELECT {COLUMNS} FROM pdf_styles WHERE id = ?1"), [id], read_style),
        None => db.query_row(&format!("SELECT {COLUMNS} FROM pdf_styles WHERE is_default"), [], read_style),
    };
    found.optional()?.ok_or_else(|| OpError::NotFound("There is no such PDF Style.".into()))
}

/// How a PDF made with a PDF Style looks.
pub(super) fn look_of(values: PdfStyleValues) -> Look {
    Look {
        text_color: values.text_color,
        heading_color: values.heading_color,
        accent_color: values.accent_color,
        body_font: values.body_font,
        heading_font: values.heading_font,
        code_font: values.code_font,
        body_size: values.body_size,
        h1_size: values.h1_size,
        h2_size: values.h2_size,
        h3_size: values.h3_size,
        cover: values.cover,
        toc: values.toc,
        toc_depth: values.toc_depth,
        footer_text: values.footer_text,
        language: values.language,
    }
}

/// The values as kept, or why they cannot be. `id` is the PDF Style they
/// are for, if it exists already.
fn valid(db: &Connection, values: PdfStyleValues, id: Option<&str>) -> Result<PdfStyleValues, OpError> {
    let invalid = |message: String| Err(OpError::InvalidInput(message));
    let name = values.name.trim().to_string();
    if name.is_empty() {
        return invalid("Enter a name for the PDF Style.".into());
    }
    if name.chars().count() > MAX_NAME_LENGTH {
        return invalid(format!("Choose a name of at most {MAX_NAME_LENGTH} characters."));
    }
    let taken: bool = db.query_row(
        "SELECT EXISTS (SELECT 1 FROM pdf_styles WHERE name = ?1 COLLATE NOCASE AND id IS NOT ?2)",
        rusqlite::params![name, id],
        |row| row.get(0),
    )?;
    if taken {
        return invalid(format!("There is already a PDF Style called {name}. Choose another name."));
    }
    let text_color = valid_color("text", &values.text_color)?;
    let heading_color = valid_color("heading", &values.heading_color)?;
    let accent_color = valid_color("accent", &values.accent_color)?;
    let body_font = valid_font_family(db, "body", &values.body_font)?;
    let heading_font = valid_font_family(db, "heading", &values.heading_font)?;
    let code_font = valid_font_family(db, "code", &values.code_font)?;
    if !(8.0..=14.0).contains(&values.body_size) {
        return invalid("Choose a body text size from 8 to 14 pt.".into());
    }
    for (level, size) in [(1, values.h1_size), (2, values.h2_size), (3, values.h3_size)] {
        if !(10.0..=36.0).contains(&size) {
            return invalid(format!("Choose a size for level {level} headings from 10 to 36 pt."));
        }
    }
    if !(1..=3).contains(&values.toc_depth) {
        return invalid("Choose a table of contents depth from 1 to 3.".into());
    }
    let footer_text = values.footer_text.map(|text| text.trim().to_string()).filter(|text| !text.is_empty());
    if let Some(text) = &footer_text {
        if text.contains(['\n', '\r']) {
            return invalid("Keep the footer text to one line.".into());
        }
        if text.chars().count() > MAX_FOOTER_LENGTH {
            return invalid(format!("Keep the footer text to at most {MAX_FOOTER_LENGTH} characters."));
        }
    }
    Ok(PdfStyleValues {
        name,
        text_color,
        heading_color,
        accent_color,
        body_font,
        heading_font,
        code_font,
        footer_text,
        ..values
    })
}

const MAX_NAME_LENGTH: usize = 200;
const MAX_FOOTER_LENGTH: usize = 200;

/// A colour as `#rrggbb` in small letters; `what` names the field.
fn valid_color(what: &str, color: &str) -> Result<String, OpError> {
    let color = color.trim();
    let hex = color.strip_prefix('#').filter(|hex| hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()));
    match hex {
        Some(_) => Ok(color.to_ascii_lowercase()),
        None => Err(OpError::InvalidInput(format!("Enter the {what} colour as #rrggbb, such as #23252b."))),
    }
}

/// The font families a PDF Style may use: those built into Turtle.
pub(super) fn font_families(_db: &Connection) -> Result<Vec<String>, OpError> {
    Ok(crate::pdf::FONT_FAMILIES.iter().map(|family| family.to_string()).collect())
}

/// A font family a PDF Style may use, as `font_families` names it; `what`
/// names the field.
pub(super) fn valid_font_family(db: &Connection, what: &str, family: &str) -> Result<String, OpError> {
    let families = font_families(db)?;
    if let Some(known) = families.iter().find(|known| known.as_str() == family.trim()) {
        return Ok(known.clone());
    }
    let listed = match families.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, others)) => format!("{} or {last}", others.join(", ")),
        None => String::new(),
    };
    Err(OpError::InvalidInput(format!("Choose a {what} font Turtle has: {listed}.")))
}

/// Only a Workspace Admin manages PDF Styles.
fn require_workspace_admin(principal: &Principal) -> Result<(), OpError> {
    super::require_workspace_admin(principal, "Only a Workspace Admin can manage PDF Styles.")
}

fn read_style(row: &rusqlite::Row) -> rusqlite::Result<PdfStyle> {
    let language: String = row.get(17)?;
    Ok(PdfStyle {
        id: row.get(0)?,
        is_default: row.get(1)?,
        values: PdfStyleValues {
            name: row.get(2)?,
            text_color: row.get(3)?,
            heading_color: row.get(4)?,
            accent_color: row.get(5)?,
            body_font: row.get(6)?,
            heading_font: row.get(7)?,
            code_font: row.get(8)?,
            body_size: row.get(9)?,
            h1_size: row.get(10)?,
            h2_size: row.get(11)?,
            h3_size: row.get(12)?,
            cover: row.get(13)?,
            toc: row.get(14)?,
            toc_depth: row.get(15)?,
            footer_text: row.get(16)?,
            language: Language::from_code(&language).unwrap_or(Language::English),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::testing::{fresh, member};

    fn standard() -> PdfStyleValues {
        PdfStyleValues {
            name: "Standard".into(),
            text_color: "#23252b".into(),
            heading_color: "#17181c".into(),
            accent_color: "#2f5f8a".into(),
            body_font: "Inter".into(),
            heading_font: "Inter".into(),
            code_font: "JetBrains Mono".into(),
            body_size: 9.6,
            h1_size: 26.0,
            h2_size: 13.5,
            h3_size: 11.0,
            cover: true,
            toc: true,
            toc_depth: 2,
            footer_text: None,
            language: Language::English,
        }
    }

    #[test]
    fn every_workspace_has_the_pdf_style_standard_as_its_default() {
        let core = fresh();
        let ken = member(&core, "Ken", false);

        let list = list_pdf_styles(&core, &ken, Nothing {}).unwrap().output;

        assert_eq!(list.pdf_styles, [PdfStyle { id: "standard".into(), is_default: true, values: standard() }]);
        assert_eq!(list.font_families, ["Inter", "Source Serif 4", "JetBrains Mono"]);
    }

    fn handout() -> PdfStyleValues {
        PdfStyleValues {
            name: "Handout".into(),
            text_color: "#111111".into(),
            heading_color: "#000000".into(),
            accent_color: "#aa3300".into(),
            body_font: "Source Serif 4".into(),
            heading_font: "Source Serif 4".into(),
            code_font: "JetBrains Mono".into(),
            body_size: 11.0,
            h1_size: 30.0,
            h2_size: 16.0,
            h3_size: 12.0,
            cover: false,
            toc: true,
            toc_depth: 1,
            footer_text: Some("Internal".into()),
            language: Language::German,
        }
    }

    fn names_and_default(core: &Core, principal: &Principal) -> Vec<(String, bool)> {
        let list = list_pdf_styles(core, principal, Nothing {}).unwrap().output;
        list.pdf_styles.into_iter().map(|style| (style.values.name, style.is_default)).collect()
    }

    #[test]
    fn a_workspace_admin_creates_edits_makes_default_and_deletes_pdf_styles() {
        let core = fresh();
        let ada = member(&core, "Ada", true);

        let created = create_pdf_style(&core, &ada, handout()).unwrap().output;
        assert_eq!((created.is_default, &created.values), (false, &handout()));
        assert_eq!(names_and_default(&core, &ada), [("Handout".into(), false), ("Standard".into(), true)]);

        let changed = PdfStyleValues { name: "Briefing".into(), body_size: 12.0, ..handout() };
        let updated = update_pdf_style(&core, &ada, ChangedPdfStyle { id: created.id.clone(), values: changed.clone() });
        assert_eq!(updated.unwrap().output.values, changed);

        set_default_pdf_style(&core, &ada, PdfStyleId { id: created.id.clone() }).unwrap();
        assert_eq!(names_and_default(&core, &ada), [("Briefing".into(), true), ("Standard".into(), false)]);

        delete_pdf_style(&core, &ada, PdfStyleId { id: "standard".into() }).unwrap();
        assert_eq!(names_and_default(&core, &ada), [("Briefing".into(), true)]);
    }

    #[test]
    fn the_default_pdf_style_cannot_be_deleted() {
        let core = fresh();
        let ada = member(&core, "Ada", true);

        let refused = delete_pdf_style(&core, &ada, PdfStyleId { id: "standard".into() }).unwrap_err();

        assert_eq!(refused.kind(), "conflict");
        assert_eq!(refused.message(), "This is the default PDF Style. Make another one the default first.");
        assert_eq!(names_and_default(&core, &ada), [("Standard".into(), true)]);
    }

    #[test]
    fn only_a_workspace_admin_manages_pdf_styles() {
        let core = fresh();
        let ken = member(&core, "Ken", false);
        let standard = || PdfStyleId { id: "standard".into() };

        let refusals = [
            create_pdf_style(&core, &ken, handout()).map(|_| ()).unwrap_err(),
            update_pdf_style(&core, &ken, ChangedPdfStyle { id: "standard".into(), values: handout() }).map(|_| ()).unwrap_err(),
            set_default_pdf_style(&core, &ken, standard()).map(|_| ()).unwrap_err(),
            delete_pdf_style(&core, &ken, standard()).map(|_| ()).unwrap_err(),
        ];

        for refused in refusals {
            assert_eq!((refused.kind(), refused.message()), ("not_permitted", "Only a Workspace Admin can manage PDF Styles."));
        }
        assert_eq!(names_and_default(&core, &ken), [("Standard".into(), true)]);
    }

    #[test]
    fn a_made_up_pdf_style_is_not_found() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let made_up = || PdfStyleId { id: "made-up".into() };

        let errors = [
            update_pdf_style(&core, &ada, ChangedPdfStyle { id: "made-up".into(), values: handout() }).map(|_| ()).unwrap_err(),
            set_default_pdf_style(&core, &ada, made_up()).map(|_| ()).unwrap_err(),
            delete_pdf_style(&core, &ada, made_up()).map(|_| ()).unwrap_err(),
        ];

        for error in errors {
            assert_eq!((error.kind(), error.message()), ("not_found", "There is no such PDF Style."));
        }
        assert_eq!(names_and_default(&core, &ada), [("Standard".into(), true)]);
    }

    #[test]
    fn invalid_values_are_refused_naming_the_field() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        create_pdf_style(&core, &ada, PdfStyleValues { name: "Briefing".into(), ..handout() }).unwrap();
        let cases = [
            (PdfStyleValues { name: "  ".into(), ..handout() }, "Enter a name for the PDF Style."),
            (PdfStyleValues { name: "briefing".into(), ..handout() }, "There is already a PDF Style called briefing. Choose another name."),
            (PdfStyleValues { text_color: "red".into(), ..handout() }, "Enter the text colour as #rrggbb, such as #23252b."),
            (PdfStyleValues { heading_color: "#12345".into(), ..handout() }, "Enter the heading colour as #rrggbb, such as #23252b."),
            (PdfStyleValues { accent_color: "#12345g".into(), ..handout() }, "Enter the accent colour as #rrggbb, such as #23252b."),
            (PdfStyleValues { body_font: "Comic Sans".into(), ..handout() }, "Choose a body font Turtle has: Inter, Source Serif 4 or JetBrains Mono."),
            (PdfStyleValues { heading_font: "".into(), ..handout() }, "Choose a heading font Turtle has: Inter, Source Serif 4 or JetBrains Mono."),
            (PdfStyleValues { code_font: "Courier".into(), ..handout() }, "Choose a code font Turtle has: Inter, Source Serif 4 or JetBrains Mono."),
            (PdfStyleValues { body_size: 7.5, ..handout() }, "Choose a body text size from 8 to 14 pt."),
            (PdfStyleValues { body_size: f64::NAN, ..handout() }, "Choose a body text size from 8 to 14 pt."),
            (PdfStyleValues { h1_size: 37.0, ..handout() }, "Choose a size for level 1 headings from 10 to 36 pt."),
            (PdfStyleValues { h2_size: 9.0, ..handout() }, "Choose a size for level 2 headings from 10 to 36 pt."),
            (PdfStyleValues { h3_size: 40.0, ..handout() }, "Choose a size for level 3 headings from 10 to 36 pt."),
            (PdfStyleValues { toc_depth: 0, ..handout() }, "Choose a table of contents depth from 1 to 3."),
            (PdfStyleValues { toc_depth: 4, ..handout() }, "Choose a table of contents depth from 1 to 3."),
            (PdfStyleValues { footer_text: Some("One\nTwo".into()), ..handout() }, "Keep the footer text to one line."),
            (PdfStyleValues { footer_text: Some("x".repeat(201)), ..handout() }, "Keep the footer text to at most 200 characters."),
        ];

        for (values, message) in cases {
            let refused = create_pdf_style(&core, &ada, values.clone()).map(|_| ()).unwrap_err();
            assert_eq!((refused.kind(), refused.message()), ("invalid_input", message), "{values:?}");
            // Changing the PDF Style Standard to them is refused alike.
            let changed = ChangedPdfStyle { id: "standard".into(), values };
            let refused = update_pdf_style(&core, &ada, changed).map(|_| ()).unwrap_err();
            assert_eq!(refused.message(), message);
        }
        assert_eq!(names_and_default(&core, &ada), [("Briefing".into(), false), ("Standard".into(), true)]);
    }

    #[test]
    fn values_are_kept_trimmed_and_an_empty_footer_is_none() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let values = PdfStyleValues { name: " Handout ".into(), text_color: "#AA3300".into(), footer_text: Some("  ".into()), ..handout() };

        let created = create_pdf_style(&core, &ada, values).unwrap().output;

        assert_eq!((created.values.name.as_str(), created.values.text_color.as_str()), ("Handout", "#aa3300"));
        assert_eq!(created.values.footer_text, None);
        // Renaming a PDF Style to its own name, in other letters, is no clash.
        let renamed = ChangedPdfStyle { id: created.id, values: PdfStyleValues { name: "HANDOUT".into(), ..handout() } };
        assert_eq!(update_pdf_style(&core, &ada, renamed).unwrap().output.values.name, "HANDOUT");
    }
}
