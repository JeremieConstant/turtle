//! The report on an import from Confluence, written as a Page beside what
//! was imported, so it can be read again later and the Pages that did not
//! come across exactly can be checked off one by one.

use super::coverage::{known_as, Coverage, Outcome, Tally};
use crate::ops::import::Problem;

/// The longest space name the report's title holds; a longer one is cut
/// short, so the title stays within what a title may be.
const NAME_IN_TITLE: usize = 120;

/// The report Page's title.
pub fn title(coverage: &Coverage, date: &str) -> String {
    let mut name: String = coverage.space.chars().take(NAME_IN_TITLE).collect();
    if name.len() < coverage.space.len() {
        name.push('…');
    }
    format!("Import report: {name} ({date})")
}

/// A page whose content may need checking, and where it is.
pub struct Checked {
    pub page_id: String,
    /// The titles of the pages and folders above it, from the top down.
    pub path: Vec<String>,
    pub tally: Tally,
}

/// The report Page's content, in Markdown. `pages` holds what each imported
/// Page's content held, and where it is.
pub fn markdown(coverage: &Coverage, pages: &[Checked], problems: &[Problem], date: &str) -> String {
    let mut out = format!(
        "This Page tells how the Confluence space **{}** came across when it was imported on {date}. To undo the import, choose **Imports** in the menu of this Area.\n\n",
        coverage.space
    );
    out += "## Summary\n\n";
    out += &format!(
        "- {} Pages, {} Folders and {} Attachments came across.\n- {} of the Pages came across exactly.\n- Of the content, {} % came across exactly and {} % at least approximately.\n\n",
        coverage.pages, coverage.folders, coverage.attachments, coverage.exact_pages, coverage.exact_percent, coverage.kept_percent
    );
    out += "## Pages to check\n\n";
    let mut to_check: Vec<(Outcome, &Checked)> = pages.iter().filter(|page| !page.tally.is_exact()).map(|page| (worst(&page.tally), page)).collect();
    // The least well kept first; among those alike, as they were imported.
    to_check.sort_by_key(|(outcome, _)| std::cmp::Reverse(*outcome));
    if to_check.is_empty() {
        out += "Every Page came across exactly.\n\n";
    } else {
        out += "Each of these Pages holds content that did not come across exactly. Check a Page off once you have looked at it.\n\n";
        for (_, page) in &to_check {
            let place = if page.path.is_empty() { String::new() } else { format!(" (in {})", page.path.join(" › ")) };
            out += &format!("- [ ] [](turtle:{}){place}: {}\n", page.page_id, what_to_check(&page.tally));
        }
        out += "\n";
    }
    out += "## What became of the content\n\n| Confluence content | How well | Found | What it became |\n| --- | --- | --- | --- |\n";
    for found in &coverage.elements {
        out += &format!("| {} | {} | {} | {} |\n", cell(&found.name), how_well(found.outcome), found.count, cell(&found.becomes));
    }
    if !coverage.not_imported.is_empty() {
        out += "\n## Not imported\n\n";
        for item in &coverage.not_imported {
            out += &format!("- {}: {}\n", item.what, item.count);
        }
    }
    if !problems.is_empty() {
        out += "\n## Did not come across\n\n";
        for problem in problems {
            out += &format!("- {}: {}\n", problem.file, problem.message);
        }
    }
    out
}

/// The least well kept outcome among what a Page held.
fn worst(tally: &Tally) -> Outcome {
    tally.0.keys().map(|name| known_as(name).0).max().unwrap_or(Outcome::Exact)
}

/// What on a Page did not come across exactly, the least well kept first:
/// "Placeholders: toc macro. Approximately: image caption ×2."
fn what_to_check(tally: &Tally) -> String {
    let mut parts = Vec::new();
    for (outcome, label) in [(Outcome::LeftOut, "Left out"), (Outcome::Placeholder, "Placeholders"), (Outcome::Approximate, "Approximately")] {
        let names: Vec<String> = tally
            .0
            .iter()
            .filter(|(name, _)| known_as(name).0 == outcome)
            .map(|(name, count)| if *count > 1 { format!("{name} ×{count}") } else { name.clone() })
            .collect();
        if !names.is_empty() {
            parts.push(format!("{label}: {}.", names.join(", ")));
        }
    }
    parts.join(" ")
}

fn how_well(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Exact => "Exactly",
        Outcome::Approximate => "Approximately",
        Outcome::Placeholder => "As a placeholder",
        Outcome::LeftOut => "Left out",
    }
}

/// Text for a table cell, which a `|` would end.
fn cell(text: &str) -> String {
    text.replace('|', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coverage_of(space: &str) -> Coverage {
        super::super::coverage::report(space.into(), (0, 0, 0), &[], Vec::new())
    }

    #[test]
    fn a_long_space_name_is_cut_short_so_the_title_stays_a_title() {
        let title = title(&coverage_of(&"Wiki ".repeat(60)), "2026-11-26");

        assert!(title.chars().count() <= crate::ops::tree::MAX_TITLE_LENGTH, "{title}");
        assert!(title.ends_with("… (2026-11-26)"), "{title}");
        assert_eq!(super::title(&coverage_of("KB"), "2026-11-26"), "Import report: KB (2026-11-26)");
    }
}
