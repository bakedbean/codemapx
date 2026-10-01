mod common;

use codemapx::html;

#[test]
fn embeds_the_map_and_title() {
    let out = html::render(&common::sample_map());
    assert!(out.contains("<title>Apply regenerated fees</title>"));
    assert!(out.contains("\"title\":\"Apply regenerated fees\""));
    assert!(!out.contains("/*MAP*/") && !out.contains("/*TITLE*/"));
}

#[test]
fn escapes_script_breakouts() {
    let mut m = common::sample_map();
    m.cards[0].what = "</script><script>alert(1)</script>".into();
    m.title = "<b>x</b>".into();
    let out = html::render(&m);
    assert_eq!(out.matches("</script>").count(), 1);
    assert!(out.contains("<title>&lt;b&gt;x&lt;/b&gt;</title>"));
}

/// Manual check only: writes target/sample-map.html to open in a browser.
#[test]
#[ignore]
fn write_sample_page() {
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/sample-map.html");
    std::fs::write(out, html::render(&common::sample_map())).unwrap();
}
