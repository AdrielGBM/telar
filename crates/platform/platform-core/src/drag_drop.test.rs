use super::*;

#[test]
fn accepts_a_type_with_a_subtype_and_parameters() {
    for name in [
        "text/plain",
        "text/plain;charset=utf-8",
        "application/x-hogar-token",
        "application/vnd.example+json; version=2",
    ] {
        assert_eq!(
            MimeType::parse(name).map(|m| m.as_str().to_owned()),
            Ok(name.to_owned())
        );
    }
}

#[test]
fn refuses_a_name_that_is_not_type_slash_subtype() {
    for name in [
        "",
        "text",
        "text/",
        "/plain",
        "te xt/plain",
        "text/pl/ain",
        "text/plain\n",
    ] {
        assert_eq!(MimeType::parse(name), Err(MimeTypeError(name.to_owned())));
    }
}

#[test]
fn a_representation_added_twice_keeps_its_place_and_takes_the_new_bytes() {
    let token = MimeType::from_static("application/x-token");
    let data = DragData::new()
        .with(token.clone(), b"old".as_slice())
        .with_text("paragraph")
        .with(token.clone(), b"new".as_slice());
    let types: Vec<&str> = data.types().map(MimeType::as_str).collect();
    assert_eq!(
        types,
        [
            "application/x-token",
            "text/plain;charset=utf-8",
            "text/plain"
        ]
    );
    assert_eq!(data.bytes(&token).map(|b| &b[..]), Some(b"new".as_slice()));
    assert_eq!(
        data.bytes(&MimeType::TEXT_PLAIN).map(|b| &b[..]),
        Some(b"paragraph".as_slice())
    );
}

#[test]
fn an_offer_names_the_first_wanted_type_it_carries() {
    let token = MimeType::from_static("application/x-token");
    let offer = DragOffer::of(&DragData::new().with_text("p"), DragActions::Copy);
    assert_eq!(
        offer.first_of(&[token, MimeType::TEXT_PLAIN_UTF8]),
        Some(&MimeType::TEXT_PLAIN_UTF8)
    );
    assert_eq!(offer.first_of(&[MimeType::URI_LIST]), None);
}

#[test]
fn the_action_chosen_is_the_preferred_one_only_where_the_source_allows_it() {
    assert_eq!(
        DragActions::CopyOrMove.choose(DropAction::Move),
        DropAction::Move
    );
    assert_eq!(DragActions::Copy.choose(DropAction::Move), DropAction::Copy);
    assert!(!DragActions::Move.allows(DropAction::Copy));
}

#[test]
fn a_drop_delivers_only_a_carried_type_under_an_allowed_action() {
    let drag = OutgoingDrag::new(DragData::new().with_text("p"), DragActions::Copy);
    let copy = DropAnswer {
        mime: MimeType::TEXT_PLAIN,
        action: DropAction::Copy,
    };
    assert_eq!(
        drag.deliver(&copy),
        Some(DroppedData {
            mime: MimeType::TEXT_PLAIN,
            bytes: Arc::from(b"p".as_slice()),
            action: DropAction::Copy,
        })
    );
    let moved = DropAnswer {
        action: DropAction::Move,
        ..copy.clone()
    };
    assert_eq!(drag.deliver(&moved), None);
    let markdown = DropAnswer {
        mime: MimeType::TEXT_MARKDOWN,
        ..copy
    };
    assert_eq!(drag.deliver(&markdown), None);
}

#[test]
fn an_icon_refuses_pixels_that_do_not_fill_its_size() {
    assert!(DragIcon::new(vec![0u8; 16], 2, 2).is_some());
    assert!(DragIcon::new(vec![0u8; 15], 2, 2).is_none());
    assert!(DragIcon::new(Vec::<u8>::new(), 0, 0).is_none());
}
