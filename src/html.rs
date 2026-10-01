//! Single-page HTML export of a map.

use crate::map::Map;

const TEMPLATE: &str = include_str!("../web/filemap.tpl.html");

pub fn render(map: &Map) -> String {
    // Escape "</" so map text can't close the <script> tag.
    let json = serde_json::to_string(map).expect("map serialize").replace("</", "<\\/");
    TEMPLATE.replace("/*TITLE*/", &escape(&map.title)).replace("/*MAP*/", &json)
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
