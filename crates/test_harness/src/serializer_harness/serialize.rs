//! Legacy html5lib token-stream serialization.

use super::*;

/// Serializes a token stream using html5lib's serializer semantics and the resolved options.
pub(super) fn serialize_tokens(tokens: &[SerializerToken], options: &EffectiveOptions) -> String {
    let mut out = String::new();
    let mut in_cdata = false;
    for token in tokens {
        match token {
            SerializerToken::Doctype {
                name,
                public_id,
                system_id,
            } => {
                out.push_str("<!DOCTYPE ");
                out.push_str(name);
                let public_id = public_id.as_deref().filter(|id| !id.is_empty());
                let system_id = system_id.as_deref().filter(|id| !id.is_empty());
                if let Some(public_id) = public_id {
                    out.push_str(&format!(" PUBLIC \"{public_id}\""));
                    if let Some(system_id) = system_id {
                        out.push_str(&format!(" \"{system_id}\""));
                    }
                } else if let Some(system_id) = system_id {
                    out.push_str(&format!(" SYSTEM \"{system_id}\""));
                }
                out.push('>');
            }
            SerializerToken::Characters(data) => {
                if in_cdata {
                    out.push_str(data);
                } else {
                    out.push_str(&html_escape(data));
                }
            }
            SerializerToken::Spaces(data) => out.push_str(data),
            SerializerToken::StartTag { name, attrs, .. }
            | SerializerToken::EmptyTag { name, attrs } => {
                out.push('<');
                out.push_str(name);
                if RCDATA_ELEMENTS.contains(&name.as_str()) && !options.escape_rcdata {
                    in_cdata = true;
                }
                for attr in attrs {
                    serialize_attribute(&mut out, name, attr, options);
                }
                if VOID_ELEMENTS.contains(&name.as_str()) && options.use_trailing_solidus {
                    if options.space_before_trailing_solidus {
                        out.push_str(" /");
                    } else {
                        out.push('/');
                    }
                }
                out.push('>');
            }
            SerializerToken::EndTag { name, .. } => {
                if RCDATA_ELEMENTS.contains(&name.as_str()) {
                    in_cdata = false;
                }
                out.push_str("</");
                out.push_str(name);
                out.push('>');
            }
            SerializerToken::Comment(data) => {
                out.push_str("<!--");
                out.push_str(data);
                out.push_str("-->");
            }
        }
    }
    out
}

pub(super) fn serialize_attribute(
    out: &mut String,
    tag_name: &str,
    attr: &SerializerAttr,
    options: &EffectiveOptions,
) {
    out.push(' ');
    out.push_str(&attr.name);
    let minimized =
        options.minimize_boolean_attributes && is_boolean_attribute(tag_name, &attr.name);
    if minimized {
        return;
    }
    let mut value = html_escape(&attr.value);
    if options.escape_lt_in_attrs && !attr.value.contains('<') {
        // html_escape already escapes `<` in this fixture dialect; keep the option explicit.
    }
    let needs_quotes =
        options.quote_attr_values || value.is_empty() || needs_legacy_quotes(&attr.value);
    if !needs_quotes {
        out.push('=');
        out.push_str(&value);
        return;
    }
    let mut quote_char = options.quote_char;
    if options.use_best_quote_char {
        if value.contains('\'') && !value.contains('"') {
            quote_char = '"';
        } else if value.contains('"') && !value.contains('\'') {
            quote_char = '\'';
        }
    }
    if quote_char == '\'' {
        value = value.replace('\'', "&#39;");
    } else {
        value = value.replace('"', "&quot;");
    }
    out.push('=');
    out.push(quote_char);
    out.push_str(&value);
    out.push(quote_char);
}
