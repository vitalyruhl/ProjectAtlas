//! `QuickScript` metadata and explicit `CALL` graph extraction.

use crate::{check_parser_iteration, empty_graph, push_relation, push_symbol_with_metadata};
use projectatlas_core::symbols::{ParserKind, RelationKind, SymbolGraph, SymbolKind};

/// Bounded explicit metadata projected into one Atlas symbol.
#[derive(Default)]
struct Metadata {
    /// Canonical script kind supplied by `@ScriptType`.
    script_type: Option<String>,
    /// Stable workspace identity supplied by `@Name`.
    name: Option<String>,
    /// One-based source line of the accepted name.
    name_line: usize,
    /// Optional human-facing description.
    description: Option<String>,
    /// Optional application or window event.
    event: Option<String>,
    /// Optional data-change or condition trigger.
    trigger: Option<String>,
    /// Optional key-script shortcut.
    shortcut: Option<String>,
}

/// Extract only the stable workspace facts shared with the native `QuickScript` LSP.
pub(crate) fn extract<E>(
    path: &str,
    language: Option<&str>,
    content: &str,
    check: &mut impl FnMut() -> Result<(), E>,
) -> Result<SymbolGraph, E> {
    check()?;
    let mut graph = empty_graph(path, language, ParserKind::Structural);
    let mut metadata = Metadata::default();
    for (line_index, line) in content.lines().enumerate() {
        check_parser_iteration(line_index, check)?;
        if let Some((key, value)) = metadata_field(line) {
            match key.as_str() {
                "scripttype" => {
                    metadata.script_type.get_or_insert(value);
                }
                "name" => {
                    if metadata.name.is_none() {
                        metadata.name_line = line_index + 1;
                        metadata.name = Some(value);
                    }
                }
                "description" => {
                    metadata.description.get_or_insert(value);
                }
                "event" => {
                    metadata.event.get_or_insert(value);
                }
                "trigger" => {
                    metadata.trigger.get_or_insert(value);
                }
                "shortcut" => {
                    metadata.shortcut.get_or_insert(value);
                }
                _ => {}
            }
        }
    }

    let owner_name = emit_metadata_symbol(&mut graph, &metadata, content.lines().count());
    let mut in_comment = false;
    for (line_index, line) in content.lines().enumerate() {
        check_parser_iteration(line_index, check)?;
        let code = code_without_brace_comments(line, &mut in_comment);
        for target in call_targets(&code) {
            push_relation(
                &mut graph,
                owner_name.as_deref().unwrap_or("<module>"),
                target,
                RelationKind::Calls,
                line_index + 1,
                code.trim(),
            );
        }
    }
    check()?;
    Ok(graph)
}

/// Parse one explicit metadata tag without interpreting surrounding comments.
fn metadata_field(line: &str) -> Option<(String, String)> {
    let candidate = line
        .trim()
        .trim_matches(|character: char| matches!(character, '{' | '}' | '>' | '<'))
        .trim();
    let tagged = candidate.strip_prefix('@')?;
    let separator = tagged.find(char::is_whitespace)?;
    let (key, value) = tagged.split_at(separator);
    let value = value.trim();
    (!value.is_empty()).then(|| (key.to_ascii_lowercase(), value.to_string()))
}

/// Emit one callable `QuickFunction` or non-callable script unit from explicit metadata.
fn emit_metadata_symbol(
    graph: &mut SymbolGraph,
    metadata: &Metadata,
    document_lines: usize,
) -> Option<String> {
    let script_type = metadata.script_type.as_deref()?;
    let name = metadata.name.as_deref()?;
    let (kind, detail) = script_identity(script_type)?;
    let mut signature = format!("@ScriptType {script_type} @Name {name}");
    for (label, value) in [
        ("Event", metadata.event.as_deref()),
        ("Trigger", metadata.trigger.as_deref()),
        ("Shortcut", metadata.shortcut.as_deref()),
    ] {
        if let Some(value) = value {
            signature.push_str(" @");
            signature.push_str(label);
            signature.push(' ');
            signature.push_str(value);
        }
    }
    push_symbol_with_metadata(
        graph,
        name,
        kind,
        metadata.name_line.max(1),
        document_lines.max(metadata.name_line).max(1),
        None,
        Some(detail),
        &signature,
        kind == SymbolKind::Function,
        metadata.description.as_deref(),
    );
    Some(name.to_string())
}

/// Map accepted `@ScriptType` spellings to their bounded Atlas identity.
fn script_identity(script_type: &str) -> Option<(SymbolKind, &'static str)> {
    let normalized = script_type
        .chars()
        .filter(|character| !matches!(character, ' ' | '-' | '_'))
        .flat_map(char::to_lowercase)
        .collect::<String>();
    match normalized.as_str() {
        "quickfunction" => Some((SymbolKind::Function, "quickscript-quick-function")),
        "datachange" => Some((SymbolKind::Module, "quickscript-data-change")),
        "condition" | "conditionalscript" => Some((SymbolKind::Module, "quickscript-condition")),
        "application" | "applicationscript" => {
            Some((SymbolKind::Module, "quickscript-application"))
        }
        "window" | "windowscript" => Some((SymbolKind::Module, "quickscript-window")),
        "keyscript" => Some((SymbolKind::Module, "quickscript-key-script")),
        "generic" => Some((SymbolKind::Module, "quickscript-generic")),
        _ => None,
    }
}

/// Remove brace comments while preserving any code before or after them.
fn code_without_brace_comments(line: &str, in_comment: &mut bool) -> String {
    let mut code = String::new();
    for character in line.chars() {
        if *in_comment {
            if character == '}' {
                *in_comment = false;
            }
        } else if character == '{' {
            *in_comment = true;
        } else if character != '}' {
            code.push(character);
        }
    }
    code
}

/// Return explicit `CALL Name(...)` targets from one comment-free source row.
fn call_targets(line: &str) -> Vec<&str> {
    let bytes = line.as_bytes();
    let mut targets = Vec::new();
    let mut cursor = 0;
    while cursor + 4 <= bytes.len() {
        let call_end = cursor + 4;
        let boundary_before = cursor == 0 || !is_identifier_byte(bytes[cursor - 1]);
        if boundary_before
            && bytes[cursor..call_end].eq_ignore_ascii_case(b"CALL")
            && bytes.get(call_end).is_some_and(u8::is_ascii_whitespace)
        {
            let mut name_start = call_end;
            while bytes.get(name_start).is_some_and(u8::is_ascii_whitespace) {
                name_start += 1;
            }
            if bytes
                .get(name_start)
                .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
            {
                let mut name_end = name_start + 1;
                while bytes
                    .get(name_end)
                    .is_some_and(|byte| is_identifier_byte(*byte))
                {
                    name_end += 1;
                }
                let mut opening_parenthesis = name_end;
                while bytes
                    .get(opening_parenthesis)
                    .is_some_and(u8::is_ascii_whitespace)
                {
                    opening_parenthesis += 1;
                }
                if bytes.get(opening_parenthesis) == Some(&b'(') {
                    targets.push(&line[name_start..name_end]);
                }
                cursor = name_end;
                continue;
            }
        }
        cursor += 1;
    }
    targets
}

/// Return whether one byte can occur inside a `QuickScript` identifier.
fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests {
    use super::extract;
    use projectatlas_core::symbols::{ParserKind, RelationKind, SymbolKind};
    use std::convert::Infallible;

    #[test]
    fn extracts_metadata_symbols_and_uncommented_calls_without_filename_conventions() {
        let source = "{>\n@ScriptType QuickFunction\n@Name NormalizeWorkspaceUri\n@Description Shared URI identity\n{<}\n{ CALL IgnoredExample(); }\nCALL RealCaller();\n";
        let graph = extract(
            "scripts/uri-normalizer.vbi",
            Some("quickscript"),
            source,
            &mut || Ok::<(), Infallible>(()),
        )
        .expect("infallible extraction");

        assert_eq!(graph.parser, ParserKind::Structural);
        assert!(graph.symbols.iter().any(|symbol| {
            symbol.name == "NormalizeWorkspaceUri"
                && symbol.kind == SymbolKind::Function
                && symbol.exported
                && symbol.documentation.as_deref() == Some("Shared URI identity")
        }));
        assert_eq!(graph.relations.len(), 1);
        assert!(graph.relations.iter().any(|relation| {
            relation.kind == RelationKind::Calls
                && relation.source_name == "NormalizeWorkspaceUri"
                && relation.target_name == "RealCaller"
        }));
    }

    #[test]
    fn classifies_non_callable_script_metadata_as_a_module() {
        let source = "{>\n@ScriptType DataChange\n@Name TemperatureChanged\n@Trigger TankTemperature\n{<}\nCALL NormalizeWorkspaceUri();\n";
        let graph = extract(
            "scripts/event-handler.vi",
            Some("quickscript"),
            source,
            &mut || Ok::<(), Infallible>(()),
        )
        .expect("infallible extraction");

        assert!(graph.symbols.iter().any(|symbol| {
            symbol.name == "TemperatureChanged"
                && symbol.kind == SymbolKind::Module
                && !symbol.exported
                && symbol.detail.as_deref() == Some("quickscript-data-change")
        }));
    }
}
