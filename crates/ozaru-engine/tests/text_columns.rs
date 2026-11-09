use ozaru_engine::document::Rectangle;
use ozaru_engine::text::page_text;

mod common;
use common::{first_page, one_page};

fn text(content: &str) -> String {
    page_text(
        &first_page(&one_page(content, "")),
        &Rectangle::new(0.0, 0.0, 612.0, 792.0),
    )
}

// Lines of 10 point text at `x`, 12 points apart from y = 700.
fn painted(rows: &[Vec<(f64, &str)>]) -> String {
    let mut content = String::new();
    for (row, pieces) in rows.iter().enumerate() {
        let y = 700.0 - 12.0 * row as f64;
        for (x, piece) in pieces {
            content.push_str(&format!("BT /F1 10 Tf {x} {y} Td ({piece}) Tj ET "));
        }
    }
    content
}

const LEFT: [&str; 5] = [
    "the left column has",
    "lines of prose with",
    "several words on each",
    "of them, as columns",
    "of an article have",
];
const RIGHT: [&str; 5] = [
    "and the right column",
    "goes on with more of",
    "the same text after",
    "the left one, from the",
    "top of the page down",
];

#[test]
fn columns_painted_row_by_row_are_read_one_after_the_other() {
    let rows: Vec<Vec<(f64, &str)>> = (0..5)
        .map(|i| vec![(72.0, LEFT[i]), (320.0, RIGHT[i])])
        .collect();
    assert_eq!(
        text(&painted(&rows)),
        format!("{}\n\n{}", LEFT.join("\n"), RIGHT.join("\n"))
    );
}

#[test]
fn three_columns_are_read_from_left_to_right() {
    let rows: Vec<Vec<(f64, &str)>> = (0..5)
        .map(|i| vec![(40.0, LEFT[i]), (230.0, RIGHT[i]), (420.0, LEFT[i])])
        .collect();
    assert_eq!(
        text(&painted(&rows)),
        format!(
            "{}\n\n{}\n\n{}",
            LEFT.join("\n"),
            RIGHT.join("\n"),
            LEFT.join("\n")
        )
    );
}

#[test]
fn a_shorter_column_ends_first() {
    let mut rows: Vec<Vec<(f64, &str)>> = (0..4)
        .map(|i| vec![(72.0, LEFT[i]), (320.0, RIGHT[i])])
        .collect();
    rows.push(vec![(72.0, LEFT[4])]);
    assert_eq!(
        text(&painted(&rows)),
        format!("{}\n\n{}", LEFT.join("\n"), RIGHT[..4].join("\n"))
    );
}

#[test]
fn a_heading_across_the_columns_stays_before_them() {
    let mut rows = vec![vec![(
        72.0,
        "A heading that runs over both of the columns of the page",
    )]];
    rows.extend((0..5).map(|i| vec![(72.0, LEFT[i]), (320.0, RIGHT[i])]));
    let found = text(&painted(&rows));
    assert!(
        found.starts_with(
            "A heading that runs over both of the columns of the page\nthe left column has\n"
        ),
        "{found}"
    );
    assert!(found.ends_with(&RIGHT.join("\n")), "{found}");
}

#[test]
fn a_table_stays_row_by_row() {
    // Short cells.
    let rows: Vec<Vec<(f64, &str)>> = (0..5)
        .map(|_| vec![(72.0, "Name"), (200.0, "Price"), (320.0, "10.00")])
        .collect();
    assert_eq!(text(&painted(&rows)), ["Name Price 10.00"; 5].join("\n"));
    // Long cells, some of two lines: the cells beside them are empty.
    let rows = vec![
        vec![
            (72.0, "Check of the line state"),
            (320.0, "Active since the start of the month"),
        ],
        vec![(320.0, "and connected with no problems at all")],
        vec![
            (72.0, "Messages sent to the clients"),
            (320.0, "More than nine hundred in the period"),
        ],
        vec![(320.0, "every one of them stored in the server")],
        vec![
            (72.0, "Messages received from them"),
            (320.0, "About the same number as were sent"),
        ],
    ];
    let found = text(&painted(&rows));
    assert!(
        found.starts_with("Check of the line state Active since"),
        "{found}"
    );
}

#[test]
fn right_to_left_text_is_left_as_it_is() {
    let rows: Vec<Vec<(f64, &str)>> = (0..5)
        .map(|i| vec![(72.0, LEFT[i]), (320.0, RIGHT[i])])
        .collect();
    // The same rows, with an Arabic letter, through ActualText.
    let arabic = painted(&rows).replacen(
        "(the left column has) Tj",
        "/Span << /ActualText <FEFF0627> >> BDC (the left column has) Tj EMC",
        1,
    );
    assert!(
        text(&arabic).starts_with("\u{0627} and the right column\n"),
        "{}",
        text(&arabic)
    );
}
