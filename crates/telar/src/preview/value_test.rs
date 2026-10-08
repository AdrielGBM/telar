use super::*;

fn round_trip(value: ArgValue) -> ArgValue {
    let text = value.to_string();
    text.parse()
        .unwrap_or_else(|e| panic!("{value:?} wrote `{text}`, which does not read back: {e}"))
}

#[test]
fn every_kind_reads_back_as_written() {
    for value in [
        ArgValue::Unset,
        ArgValue::Bool(true),
        ArgValue::Bool(false),
        ArgValue::Int(0),
        ArgValue::Int(-42),
        ArgValue::Int(i128::from(u64::MAX)),
        ArgValue::Float(1.5),
        ArgValue::Float(-0.25),
        ArgValue::Float(1e-7),
        ArgValue::Float(1e300),
        ArgValue::Float(f64::INFINITY),
        ArgValue::Float(f64::NEG_INFINITY),
        ArgValue::Text(String::new()),
        ArgValue::Text("Save".into()),
        ArgValue::Text("quote \" slash \\ line\nreturn\rtab\tbell\u{7}".into()),
        ArgValue::Text("señal ✓ 日本".into()),
        ArgValue::Color(Color::from_rgba_u8(255, 136, 0, 255)),
        ArgValue::Color(Color::from_rgba_u8(1, 2, 3, 128)),
        ArgValue::Color(Color::rgba(0.1, 0.2, 0.3, 0.4)),
        ArgValue::Choice("Large".into()),
        ArgValue::Choice("_private".into()),
    ] {
        assert_eq!(round_trip(value.clone()), value);
    }
}

#[test]
fn nan_reads_back_as_nan() {
    let ArgValue::Float(value) = round_trip(ArgValue::Float(f64::NAN)) else {
        panic!("NaN read back as another kind");
    };
    assert!(value.is_nan());
}

/// A whole float has to stay a float, or a control set to `2.0` would come back from a link as an integer.
#[test]
fn a_whole_float_stays_a_float() {
    assert_eq!(ArgValue::Float(2.0).to_string(), "2.0");
    assert_eq!(round_trip(ArgValue::Float(2.0)), ArgValue::Float(2.0));
}

#[test]
fn the_text_form_is_the_documented_one() {
    assert_eq!(ArgValue::Unset.to_string(), "none");
    assert_eq!(ArgValue::Int(-12).to_string(), "-12");
    assert_eq!(ArgValue::Text("a \"b\"".into()).to_string(), r#""a \"b\"""#);
    assert_eq!(
        ArgValue::Color(Color::from_rgba_u8(255, 136, 0, 255)).to_string(),
        "#ff8800"
    );
    assert_eq!(
        ArgValue::Color(Color::from_rgba_u8(255, 136, 0, 128)).to_string(),
        "#ff880080"
    );
    assert_eq!(
        ArgValue::Color(Color::rgba(0.1, 0.2, 0.3, 1.0)).to_string(),
        "rgba(0.1, 0.2, 0.3, 1.0)"
    );
    assert_eq!(ArgValue::Choice("Large".into()).to_string(), "Large");
}

#[test]
fn shorthand_a_person_might_type_is_read() {
    assert_eq!("+5".parse(), Ok(ArgValue::Int(5)));
    assert_eq!(".5".parse(), Ok(ArgValue::Float(0.5)));
    assert_eq!(
        "#f80".parse(),
        Ok(ArgValue::Color(Color::from_rgba_u8(255, 136, 0, 255)))
    );
    assert_eq!(
        "rgba( 1 ,0, 0,1 )".parse(),
        Ok(ArgValue::Color(Color::rgba(1.0, 0.0, 0.0, 1.0)))
    );
}

#[test]
fn malformed_text_is_refused() {
    for text in [
        "",
        "-",
        "\"open",
        "\"bad \\q escape\"",
        "\"inner \" quote\"",
        "#12345",
        "rgba(1, 2, 3)",
        "rgba(1, 2, 3, 4, 5)",
        "two words",
        "a-b",
    ] {
        assert!(
            text.parse::<ArgValue>().is_err(),
            "`{text}` should not read as a value"
        );
    }
}

#[test]
fn serde_carries_the_text_form() {
    let value = ArgValue::Text("Save".into());
    let json = serde_json::to_string(&value).unwrap();
    assert_eq!(json, r#""\"Save\"""#);
    assert_eq!(serde_json::from_str::<ArgValue>(&json).unwrap(), value);

    let map: std::collections::BTreeMap<String, ArgValue> = [
        ("ghost".to_string(), ArgValue::Bool(true)),
        ("size".to_string(), ArgValue::Choice("Large".into())),
    ]
    .into();
    let json = serde_json::to_string(&map).unwrap();
    assert_eq!(json, r#"{"ghost":"true","size":"Large"}"#);
    assert_eq!(
        serde_json::from_str::<std::collections::BTreeMap<String, ArgValue>>(&json).unwrap(),
        map
    );
}

#[test]
fn serde_refuses_malformed_text() {
    assert!(serde_json::from_str::<ArgValue>(r#""two words""#).is_err());
}
