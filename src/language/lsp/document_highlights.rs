// Parsed by `convert::document_highlights`; included into the module.
/// Highlight ranges from a `textDocument/documentHighlight` result.
///
/// Positions stay in the server's coordinate units; the app converts them with
/// the document's negotiated encoding before storing character ranges.
pub fn document_highlights(value: &Value) -> Vec<((usize, usize), (usize, usize))> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let range = item.get("range")?;
            let start = range.get("start")?;
            let end = range.get("end")?;
            Some((
                (
                    start.get("line")?.as_u64()? as usize,
                    start.get("character")?.as_u64()? as usize,
                ),
                (
                    end.get("line")?.as_u64()? as usize,
                    end.get("character")?.as_u64()? as usize,
                ),
            ))
        })
        .collect()
}
