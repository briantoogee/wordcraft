//! Layout rules measured against Word for Mac's PDFs of recruitment CV templates, as synthetic
//! documents. No external documents or image assets are used.
use super::*;
use wordcraft_doc::para::{Anchor, Float, FloatAlign, Wrap};
use wordcraft_doc::props::{CharProps, HeightRule, LineSpacing, TabAlign, TabStop, TableFloat, TextFrame};
use wordcraft_doc::{PartKind, para_block};

fn picture(name: &str, w: f32, h: f32, float: Float) -> InlineObject {
    InlineObject::Image { media: name.into(), w, h, alt: name.into(), float, crop: [0.0; 4], fx: Default::default(), ole: None }
}

/// The body paragraph at `block`'s first piece: its page, item index, top and layout.
fn piece(l: &DocLayout, block: u32) -> (usize, usize, f32, Arc<ParaLayout>) {
    l.pages
        .iter()
        .enumerate()
        .find_map(|(pi, page)| {
            page.items.iter().enumerate().find_map(|(ii, it)| match it {
                Placed::Lines { story: StoryRef::Body, path, para, y, .. } if *path == Path::top(block as usize) => Some((pi, ii, *y, para.clone())),
                _ => None,
            })
        })
        .unwrap()
}

/// The bottom of the lowest body line on page `pi`.
fn body_text_bottom(l: &DocLayout, pi: usize) -> f32 {
    l.pages[pi]
        .items
        .iter()
        .filter_map(|it| match it {
            Placed::Lines { story: StoryRef::Body, para, y, l0, l1, .. } => crate::page::item_bottom(*y, para, *l0, *l1),
            _ => None,
        })
        .fold(f32::MIN, f32::max)
}

/// A right tab stop at the right margin, on a line narrowed by a picture floating at the right:
/// Word ends the tab's text against the picture, not under it (a CV's role dates beside a
/// sidebar ended 0.6 pt short of the sidebar table in Word; they ran on to the margin).
#[test]
fn a_right_tab_beside_a_float_ends_its_text_at_the_float() {
    crate::test_fonts();
    let mut d = Document::from_text("Advanced Tech Solutions\tJanuary 2018 – Present");
    let tw = d.last_section.text_width();
    d.para_mut(StoryRef::Body, &Path::top(0)).unwrap().props.tabs =
        Some(vec![TabStop { pos: tw - 2.0, align: TabAlign::Right, leader: Default::default() }]);
    let float = Float { wrap: Wrap::Square, h_rel: Anchor::Margin, h_align: Some(FloatAlign::End), v_rel: Anchor::Paragraph, ..Default::default() };
    d.insert_object(&Pos::body(0, 0), picture("sidebar", 128.0, 200.0, float), &Default::default()).unwrap();
    let l = lay(&d);
    let (_, _, _, para) = piece(&l, 0);
    let line = &para.lines[0];
    assert!(line.right < tw - 120.0, "the line is narrowed by the picture: {} of {tw}", line.right);
    let end = *line.xs.last().unwrap();
    assert!((end - line.right).abs() < 0.5, "the dates end at the picture's edge {}, not at {end}", line.right);
}

/// A footer made of a text frame placed at the bottom of the margins and taller than the space
/// below the bottom margin: Word ends the body above the frame (a CV footer's 25 pt frame took
/// one line off every page); the frame took no room and the text ran under it.
#[test]
fn a_footer_frame_reaching_into_the_body_ends_the_body_above_it() {
    crate::test_fonts();
    let lines: Vec<String> = (1..=80).map(|i| format!("Line {i}")).collect();
    let mut d = Document::from_text(&lines.join("\n"));
    let tw = d.last_section.text_width();
    let mut framed = wordcraft_doc::Paragraph::with_text("Represented by an agency", Default::default());
    framed.props.frame = Some(TextFrame {
        w: Some(tw),
        h: Some(60.0),
        h_anchor: "margin".into(),
        v_anchor: "margin".into(),
        x_align: Some("left".into()),
        y_align: Some("bottom".into()),
        ..Default::default()
    });
    let id = d.add_part(PartKind::Footer, vec![para_block(framed)]);
    d.last_section.footers.default = Some(id);
    let s = &d.last_section;
    let frame_top = s.page_h - s.margin_bottom - 60.0;
    let l = lay(&d);
    let bottom = body_text_bottom(&l, 0);
    assert!(bottom <= frame_top + 0.01, "the body ends above the frame at {frame_top}, not at {bottom}");
    assert!(bottom > frame_top - 30.0, "the body still fills the page down to the frame: {bottom}");
}

/// A manual line break formatted larger than the text before it (an unformatted `w:br` run at
/// the default size after 7 pt footer text): Word sizes the line by its text.
#[test]
fn a_line_break_does_not_make_a_line_of_text_taller() {
    crate::test_fonts();
    let small = CharProps { size: Some(7.0), ..Default::default() };
    let line_heights = |break_size: f32| {
        let mut d = Document::from_text("");
        let at = d.insert_text(&Pos::body(0, 0), "Represented by", &small).unwrap();
        let at =
            d.insert_text(&at, &wordcraft_doc::para::LINE_BREAK.to_string(), &CharProps { size: Some(break_size), ..Default::default() }).unwrap();
        d.insert_text(&at, "Call the office", &small).unwrap();
        let (_, _, _, para) = piece(&lay(&d), 0);
        para.lines.iter().map(|l| l.height).collect::<Vec<_>>()
    };
    let plain = line_heights(7.0);
    let big = line_heights(20.0);
    assert_eq!(plain.len(), 2);
    assert!((big[0] - plain[0]).abs() < 0.01, "a 20 pt break leaves the 7 pt line {} tall, not {}", plain[0], big[0]);
}

/// A picture alone in a paragraph with less than single spacing (`w:line="0"`, a header logo):
/// Word takes (1 - m) of the paragraph font's height off the line's top and the picture rises
/// that far above it (the logo's top on the page's edge, not the space before further down).
#[test]
fn a_picture_alone_under_less_than_single_spacing_rises_above_its_line() {
    crate::test_fonts();
    let laid = |spacing: f32, with_picture: bool| {
        let mut d = Document::from_text("");
        if with_picture {
            d.insert_object(&Pos::body(0, 0), picture("logo", 400.0, 60.0, Float::default()), &Default::default()).unwrap();
        }
        d.para_mut(StoryRef::Body, &Path::top(0)).unwrap().props.line_spacing = Some(LineSpacing::Multiple(spacing));
        let l = lay(&d);
        let (pi, _, y, para) = piece(&l, 0);
        let rect = l.pages[pi].items.iter().find_map(|it| if let Placed::Object { rect, .. } = it { Some(*rect) } else { None });
        (y, para.lines[0].height, rect)
    };
    let font = laid(1.0, false).1;
    let (_, single, _) = laid(1.0, true);
    let (y, half, rect) = laid(0.5, true);
    assert!((single - 60.0).abs() < 0.01, "a single-spaced picture line is as tall as the picture: {single}");
    assert!((half - (60.0 - 0.5 * font)).abs() < 0.05, "half spacing takes half the font's {font} pt off: {half}");
    let rect = rect.unwrap();
    assert!((rect.y - (y - 0.5 * font)).abs() < 0.05, "the picture rises above the line's top {y}: {}", rect.y);
    assert!((rect.bottom() - (y + half)).abs() < 0.05, "and ends on the line's bottom: {}", rect.bottom());
}

/// A picture in front of the text is drawn over every paragraph on its page, also those after
/// its anchor (a CV banner anchored in the first paragraph covered the text below it in Word).
#[test]
fn an_in_front_picture_is_drawn_over_the_text_after_its_anchor() {
    crate::test_fonts();
    let mut d = Document::from_text("Anchor\nSummary below the banner\nQualifications");
    let float = Float { wrap: Wrap::InFrontOfText, h_rel: Anchor::Margin, v_rel: Anchor::Paragraph, ..Default::default() };
    d.insert_object(&Pos::body(0, 0), picture("banner", 300.0, 80.0, float), &Default::default()).unwrap();
    let l = lay(&d);
    let items = &l.pages[0].items;
    let banner = items.iter().position(|it| matches!(it, Placed::Image { media, .. } if media.as_str() == "banner")).unwrap();
    for block in [1, 2] {
        let (_, text, ..) = piece(&l, block);
        assert!(banner > text, "the banner (item {banner}) is drawn after paragraph {block} (item {text})");
    }
}

/// A floating table placed on the margins, taller than the room a tall footer leaves the body
/// but within the margins (a CV sidebar 680 pt tall, 693 pt between the margins, 677 pt above
/// the footer): Word keeps it floating, with the text beside it on its page. It was laid out as
/// a table in the text, and everything after it went to the next page.
#[test]
fn a_margin_anchored_table_reaching_into_a_tall_footer_stays_floating() {
    crate::test_fonts();
    let mut d = Document::from_text("Profile summary beside the sidebar");
    d.last_section.margin_bottom = 36.0;
    d.last_section.footer = 36.0;
    let id = d.add_part(PartKind::Footer, vec![para_block(wordcraft_doc::Paragraph::with_text("1", Default::default()))]);
    d.last_section.footers.default = Some(id);
    let s = &d.last_section;
    let (text_h, top) = (s.text_height(), s.margin_top);
    let mut t = Table::new(1, 1, 128.0);
    t.rows[0].props.height = Some(text_h - 8.0);
    t.rows[0].props.height_rule = HeightRule::Exact;
    t.props.float = Some(TableFloat {
        h_rel: Anchor::Margin,
        v_rel: Anchor::Margin,
        h_align: Some(FloatAlign::End),
        v_align: Some(FloatAlign::Start),
        overlap: true,
        ..Default::default()
    });
    d.insert_block(StoryRef::Body, &Path::top(0), Block::Table(t)).unwrap();
    let l = lay(&d);
    // The footer does take the body's room: the table is taller than what it leaves.
    assert!(l.pages[0].body.h < text_h - 8.0, "the footer leaves the body {} pt", l.pages[0].body.h);
    assert_eq!(l.pages[0].float_tables.len(), 1, "the table floats");
    let (pi, _, y, _) = piece(&l, 1);
    assert_eq!((pi, l.pages.len()), (0, 1), "the text stays beside the table on page 1");
    assert!((y - top).abs() < 1.0, "at the top of the body: {y}");
}

/// The last line of a page under a footer taller than the bottom margin: as at the margin, only
/// its text has to fit above the footer and its extra leading may run into it (Word kept a
/// 1.15-spaced 11pt line on the page with its text 0.8pt above a small-print footer and its
/// leading 1.1pt into it, on every page of two CV templates; the line went to the next page).
#[test]
fn a_multiple_spaced_lines_leading_may_run_into_a_tall_footer() {
    crate::test_fonts();
    for multiple in [1.15, 2.0] {
        let mut past_footer = 0;
        for shift in (0..28).step_by(2) {
            let mut d = Document::from_text(&"Several words wrap onto many lines of a long paragraph. ".repeat(60));
            d.last_section.margin_bottom = 36.0;
            d.last_section.footer = 36.0;
            let small = wordcraft_doc::Paragraph::with_text("This CV is provided in confidence.", Default::default());
            let id = d.add_part(PartKind::Footer, vec![para_block(small.clone()), para_block(small.clone()), para_block(small)]);
            d.last_section.footers.default = Some(id);
            if let Some(Block::Para(p)) = d.body.get_mut(0).map(Arc::make_mut) {
                p.props.line_spacing = Some(LineSpacing::Multiple(multiple));
                p.props.space_before = Some(shift as f32);
                p.props.widow_control = Some(false);
            }
            let l = lay(&d);
            let bottom = l.pages[0].body.bottom();
            assert!(bottom < d.last_section.page_h - 36.0 - 1.0, "the footer reaches above the bottom margin: body ends at {bottom}");
            let Some(Placed::Lines { y, para, l0, l1, .. }) =
                l.pages[0].items.iter().find(|i| matches!(i, Placed::Lines { story: StoryRef::Body, .. }))
            else {
                panic!("no body lines on page 1")
            };
            let (first, last, next) = (&para.lines[*l0], &para.lines[*l1 - 1], &para.lines[*l1]);
            let top = y + last.top - first.top;
            let text_end = top + (last.baseline - last.top) + last.text_desc;
            assert!(text_end <= bottom + 0.01, "{multiple} shift {shift}: the last line's text ends at {text_end}, in the footer at {bottom}");
            let next_text_end = top + last.height + (next.baseline - next.top) + next.text_desc;
            assert!(
                next_text_end > bottom + 0.01,
                "{multiple} shift {shift}: the next line's text would end at {next_text_end}, above the footer at {bottom}"
            );
            if top + last.height > bottom + 0.01 {
                past_footer += 1;
            }
        }
        assert!(past_footer > 0, "{multiple}: some shift should leave the last line's leading in the footer");
    }
}
