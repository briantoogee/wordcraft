//! Reading rules measured against Word for Mac's PDFs of recruitment CV templates. Fixtures are
//! hand-written packages; no file produced by Word is used.

use std::io::Write;

use wordcraft_doc::props::CharProps;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// A package of `body`, with `styles` as its styles part when given.
fn docx(body: &str, styles: Option<&str>) -> Vec<u8> {
    let root = r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{W}"><w:body>{body}</w:body></w:document>"#);
    let rels = r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let mut parts: Vec<(&str, String)> = vec![("_rels/.rels", root.into()), ("word/document.xml", doc)];
    if let Some(s) = styles {
        parts.push(("word/_rels/document.xml.rels", rels.into()));
        parts.push(("word/styles.xml", format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="{W}">{s}</w:styles>"#)));
    }
    let mut zw = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, body) in parts {
        zw.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zw.write_all(body.as_bytes()).unwrap();
    }
    zw.finish().unwrap().into_inner()
}

/// Word keeps a paragraph border's space in five bits: up to 31 pt as given, more as the
/// remainder by 32. Measured: `w:space="100"` is drawn 4 pt from the text, `"40"` 8 pt (three
/// CV templates and a page border's `"201"` at 9 pt, all to within 0.4 pt).
#[test]
fn a_border_space_past_31_points_keeps_its_low_five_bits() {
    for (given, drawn) in [("0", 0.0), ("7", 7.0), ("31", 31.0), ("32", 0.0), ("40", 8.0), ("100", 4.0), ("201", 9.0)] {
        let body = format!(
            r#"<w:p><w:pPr><w:pBdr><w:top w:val="single" w:sz="6" w:space="{given}" w:color="000000"/></w:pBdr></w:pPr><w:r><w:t>x</w:t></w:r></w:p>"#
        );
        let d = wordcraft_docx::read(&docx(&body, None)).unwrap();
        let p = d.body[0].as_para().unwrap();
        let top = p.props.borders.as_ref().and_then(|b| b.top).unwrap();
        assert_eq!(top.space, drawn, "w:space={given}");
    }
}

/// Document defaults that name no font (`<w:rPrDefault/>`, as the docx JavaScript library
/// writes them): Word sets the text in Times New Roman 10 pt, not the application's body font.
#[test]
fn empty_document_defaults_mean_times_new_roman_ten_points() {
    let body = r#"<w:p><w:r><w:t>Terms of business</w:t></w:r></w:p>"#;
    for defaults in [
        "<w:docDefaults><w:rPrDefault/><w:pPrDefault/></w:docDefaults>",
        "<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:eastAsia=\"SimSun\"/></w:rPr></w:rPrDefault></w:docDefaults>",
    ] {
        let d = wordcraft_docx::read(&docx(body, Some(defaults))).unwrap();
        let c = d.styles.resolve_char(None, &CharProps::default());
        assert_eq!(c.font, "Times New Roman", "{defaults}");
        assert_eq!(c.size, 10.0, "{defaults}");
    }
    // A default that names its font keeps it.
    let named = r#"<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/></w:rPr></w:rPrDefault></w:docDefaults>"#;
    let d = wordcraft_docx::read(&docx(body, Some(named))).unwrap();
    assert_eq!(d.styles.resolve_char(None, &CharProps::default()).font, "Arial");
}

/// Paragraphs nested in a paragraph (invalid, written by some generators): Word reads each as a
/// paragraph of its own after it (a CV header's two nested empty paragraphs took two lines).
#[test]
fn paragraphs_nested_in_a_paragraph_follow_it() {
    let body = r#"<w:p><w:r><w:t>a</w:t></w:r><w:p/><w:p><w:r><w:t>b</w:t></w:r></w:p></w:p><w:p><w:r><w:t>c</w:t></w:r></w:p>"#;
    let d = wordcraft_docx::read(&docx(body, None)).unwrap();
    let texts: Vec<&str> = d.body.iter().filter_map(|b| b.as_para()).map(|p| p.text.as_str()).collect();
    assert_eq!(texts, ["a", "", "b", "c"]);
}

/// Page borders measured from the text (`w:offsetFrom` left out or "text"): Word draws a line
/// 9pt from the text (`w:space="201"`, kept to its low five bits) outside a 37pt margin about
/// 27.5pt from the page's edge, where the model measures page borders from. Measured from the
/// page, the space stays. Both come back as they were after a save.
#[test]
fn page_borders_measured_from_the_text_sit_outside_the_margins() {
    let sect = |from: &str| {
        format!(
            r#"<w:p/><w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="540" w:right="740" w:bottom="540" w:left="740" w:header="708" w:footer="500" w:gutter="0"/><w:pgBorders{from}><w:top w:val="single" w:sz="8" w:color="000000"/><w:left w:val="single" w:sz="8" w:space="201" w:color="000000"/></w:pgBorders></w:sectPr>"#
        )
    };
    let spaces = |d: &wordcraft_doc::Document| {
        let b = d.last_section.page_borders.unwrap();
        (b.top.unwrap().space, b.left.unwrap().space)
    };
    for from in ["", r#" w:offsetFrom="text""#] {
        let d = wordcraft_docx::read(&docx(&sect(from), None)).unwrap();
        assert_eq!(spaces(&d), (27.0 - 0.5, 37.0 - 9.0 - 0.5), "{from}");
        let again = wordcraft_docx::read(&wordcraft_docx::write(&d).unwrap()).unwrap();
        assert_eq!(spaces(&again), spaces(&d), "{from}: a save keeps them");
    }
    let d = wordcraft_docx::read(&docx(&sect(r#" w:offsetFrom="page""#), None)).unwrap();
    assert_eq!(spaces(&d), (0.0, 9.0));
}
