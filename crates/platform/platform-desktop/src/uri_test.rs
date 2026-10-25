use super::*;

fn method_error(name: &str) -> zbus::Error {
    zbus::Error::MethodError(
        zbus::names::OwnedErrorName::try_from(name).unwrap(),
        None,
        zbus::message::Message::method_call("/", "Ping")
            .unwrap()
            .build(&())
            .unwrap(),
    )
}

fn is_refused(error: zbus::Error) -> bool {
    matches!(PortalError::from(error), PortalError::Refused(_))
}

#[test]
fn only_an_explicit_refusal_is_final() {
    assert!(is_refused(zbus::Error::FDO(Box::new(
        zbus::fdo::Error::AccessDenied("no".into())
    ))));
    assert!(is_refused(zbus::Error::FDO(Box::new(
        zbus::fdo::Error::InvalidArgs("bad".into())
    ))));
    for name in [
        "org.freedesktop.DBus.Error.AccessDenied",
        "org.freedesktop.DBus.Error.InvalidArgs",
        "org.freedesktop.portal.Error.NotAllowed",
    ] {
        assert!(is_refused(method_error(name)), "{name}");
    }
}

#[test]
fn a_portal_that_cannot_answer_falls_back_to_the_launcher() {
    for name in [
        "org.freedesktop.DBus.Error.ServiceUnknown",
        "org.freedesktop.DBus.Error.NameHasNoOwner",
        "org.freedesktop.DBus.Error.NoReply",
        "org.freedesktop.DBus.Error.TimedOut",
        "org.freedesktop.DBus.Error.Spawn.ChildExited",
        "org.freedesktop.DBus.Error.Spawn.Failed",
        "org.freedesktop.DBus.Error.Failed",
    ] {
        assert!(!is_refused(method_error(name)), "{name}");
    }
    for error in [
        zbus::Error::FDO(Box::new(zbus::fdo::Error::ServiceUnknown("x".into()))),
        zbus::Error::FDO(Box::new(zbus::fdo::Error::NoReply("x".into()))),
        zbus::Error::FDO(Box::new(zbus::fdo::Error::TimedOut("x".into()))),
        zbus::Error::FDO(Box::new(zbus::fdo::Error::Failed("x".into()))),
        zbus::Error::InputOutput(std::io::Error::from(std::io::ErrorKind::BrokenPipe).into()),
        zbus::Error::Failure("anything else".into()),
    ] {
        assert!(!is_refused(error));
    }
}
