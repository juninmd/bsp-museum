//! Construtores de JSON Schema para os `inputSchema` das ferramentas e o validador do
//! subconjunto que usamos (object/properties/required/additionalProperties=false,
//! type, enum, minimum/maximum, minLength/maxLength). O schema é a única fonte da
//! verdade: o que ele diz é o que `tools/call` confere antes de chamar a ferramenta.

use serde_json::{json, Map, Value};

/// Teto de tamanho de qualquer string de entrada (caminhos, nomes, filtros).
pub const MAX_INPUT_STRING: usize = 4096;

pub fn string(desc: &str) -> Value {
    json!({"type": "string", "description": desc, "maxLength": MAX_INPUT_STRING})
}

pub fn string_enum(desc: &str, values: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": values})
}

pub fn integer(desc: &str, min: i64, max: i64, default: i64) -> Value {
    json!({"type": "integer", "description": desc, "minimum": min, "maximum": max, "default": default})
}

pub fn boolean(desc: &str, default: bool) -> Value {
    json!({"type": "boolean", "description": desc, "default": default})
}

/// Objeto de entrada: `required` lista as chaves obrigatórias; chave desconhecida é recusada.
pub fn object(props: Vec<(&str, Value)>, required: &[&str]) -> Value {
    let mut p = Map::new();
    for (k, v) in props {
        p.insert(k.to_string(), v);
    }
    json!({"type": "object", "properties": p, "required": required, "additionalProperties": false})
}

fn type_ok(ty: &str, v: &Value) -> bool {
    match ty {
        "string" => v.is_string(),
        "boolean" => v.is_boolean(),
        "integer" => v.is_i64() || v.is_u64() || v.as_f64().is_some_and(|f| f.fract() == 0.0 && f.is_finite()),
        "number" => v.is_number(),
        "array" => v.is_array(),
        "object" => v.is_object(),
        _ => true,
    }
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Confere `args` contra `schema`. A mensagem de erro diz qual campo e por quê.
pub fn validate(schema: &Value, args: &Value) -> Result<(), String> {
    let empty = Map::new();
    let obj = match args {
        Value::Object(m) => m,
        Value::Null => &empty,
        other => return Err(format!("os argumentos devem ser um objeto, veio {}", type_name(other))),
    };
    let props = schema.get("properties").and_then(Value::as_object).unwrap_or(&empty);
    if let Some(req) = schema.get("required").and_then(Value::as_array) {
        for k in req.iter().filter_map(Value::as_str) {
            if obj.get(k).map_or(true, Value::is_null) {
                return Err(format!("argumento obrigatório ausente: {k}"));
            }
        }
    }
    for (k, v) in obj {
        let Some(p) = props.get(k) else {
            if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
                let known: Vec<&str> = props.keys().map(String::as_str).collect();
                return Err(format!("argumento desconhecido: {k} (aceitos: {})", known.join(", ")));
            }
            continue;
        };
        if v.is_null() {
            continue; // opcional passado como null = ausente
        }
        if let Some(ty) = p.get("type").and_then(Value::as_str) {
            if !type_ok(ty, v) {
                return Err(format!("{k}: esperado {ty}, veio {}", type_name(v)));
            }
        }
        if let Some(en) = p.get("enum").and_then(Value::as_array) {
            if !en.contains(v) {
                let list: Vec<String> = en.iter().filter_map(Value::as_str).map(str::to_string).collect();
                return Err(format!("{k}: valor fora da lista ({})", list.join(", ")));
            }
        }
        if let Some(n) = v.as_f64() {
            if let Some(min) = p.get("minimum").and_then(Value::as_f64) {
                if n < min {
                    return Err(format!("{k}: mínimo {min}, veio {n}"));
                }
            }
            if let Some(max) = p.get("maximum").and_then(Value::as_f64) {
                if n > max {
                    return Err(format!("{k}: máximo {max}, veio {n}"));
                }
            }
        }
        if let Some(s) = v.as_str() {
            let max = p.get("maxLength").and_then(Value::as_u64).unwrap_or(MAX_INPUT_STRING as u64);
            if s.chars().count() as u64 > max {
                return Err(format!("{k}: texto passa de {max} caracteres"));
            }
            if let Some(min) = p.get("minLength").and_then(Value::as_u64) {
                if (s.chars().count() as u64) < min {
                    return Err(format!("{k}: texto menor que {min} caracteres"));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sch() -> Value {
        object(
            vec![
                ("path", string("caminho")),
                ("limit", integer("máx", 1, 100, 10)),
                ("modo", string_enum("modo", &["a", "b"])),
                ("flag", boolean("f", false)),
            ],
            &["path"],
        )
    }

    #[test]
    fn aceita_valido_e_null_opcional() {
        assert!(validate(&sch(), &json!({"path": "x", "limit": 5, "modo": "a", "flag": true})).is_ok());
        assert!(validate(&sch(), &json!({"path": "x", "limit": null})).is_ok());
        assert!(validate(&sch(), &json!({"path": "x", "limit": 5.0})).is_ok(), "5.0 é inteiro");
    }

    #[test]
    fn recusa_ausente_tipo_faixa_enum_e_desconhecido() {
        let e = |v: Value| validate(&sch(), &v).unwrap_err();
        assert!(e(json!({})).contains("obrigatório"));
        assert!(e(Value::Null).contains("obrigatório"));
        assert!(e(json!({"path": 3})).contains("esperado string"));
        assert!(e(json!({"path": "x", "limit": 0})).contains("mínimo"));
        assert!(e(json!({"path": "x", "limit": 101})).contains("máximo"));
        assert!(e(json!({"path": "x", "limit": 1.5})).contains("esperado integer"));
        assert!(e(json!({"path": "x", "modo": "z"})).contains("fora da lista"));
        assert!(e(json!({"path": "x", "extra": 1})).contains("desconhecido"));
        assert!(e(json!([1])).contains("objeto"));
        assert!(e(json!({"path": "a".repeat(5000)})).contains("passa de"));
    }
}
