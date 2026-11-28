use super::*;

fn id(seed: char) -> DeliveryId {
    DeliveryId::try_from(seed.to_string().repeat(32)).unwrap_or_else(|error| panic!("{error}"))
}

fn deliveries_in(directory: &tempfile::TempDir) -> Deliveries {
    Deliveries::beside(&directory.path().join("settings.json"))
}

#[test]
fn only_the_first_claim_of_a_delivery_may_paste() -> io::Result<()> {
    let directory = tempfile::tempdir()?;
    assert!(deliveries_in(&directory).claim(&id('a'))?);
    // The reply was lost: the shell asks again, maybe of a restarted daemon.
    let restarted = deliveries_in(&directory);
    assert!(!restarted.claim(&id('a'))?);
    assert!(!restarted.claim(&id('a'))?);
    // The next Return is its own delivery.
    assert!(restarted.claim(&id('b'))?);
    Ok(())
}

#[test]
fn only_one_of_many_racing_claims_may_paste() -> io::Result<()> {
    let directory = tempfile::tempdir()?;
    let claims: Vec<_> = (0..8)
        .map(|_| {
            let deliveries = deliveries_in(&directory);
            std::thread::spawn(move || deliveries.claim(&id('c')))
        })
        .collect();
    let mut first = 0;
    for claim in claims {
        if claim.join().map_err(|_| io::Error::other("a claim panicked"))?? {
            first += 1;
        }
    }
    assert_eq!(first, 1);
    Ok(())
}

#[test]
fn an_unreadable_file_lets_nothing_paste_until_it_is_replaced() -> io::Result<()> {
    let directory = tempfile::tempdir()?;
    let deliveries = deliveries_in(&directory);
    assert!(deliveries.claim(&id('a'))?);
    fs::write(directory.path().join("deliveries.json"), "not json")?;
    assert!(!deliveries.claim(&id('a'))?, "the claim may have been in it");
    assert!(deliveries.claim(&id('d'))?, "the file was replaced");
    // A file that cannot be written refuses the claim, so nothing pastes.
    let blocked = Deliveries::open(directory.path().join("blocked.json"));
    fs::create_dir(directory.path().join("blocked.json"))?;
    assert!(blocked.claim(&id('e')).is_err());
    Ok(())
}

#[test]
fn the_file_keeps_only_recent_ids() -> io::Result<()> {
    let directory = tempfile::tempdir()?;
    let deliveries = deliveries_in(&directory);
    for n in 0..=MAX_KEPT {
        let id = DeliveryId::try_from(format!("{n:032x}")).map_err(io::Error::other)?;
        assert!(deliveries.claim(&id)?);
    }
    let text = fs::read_to_string(directory.path().join("deliveries.json"))?;
    let file: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(file["entries"].as_array().map(Vec::len), Some(MAX_KEPT));
    assert_eq!(file["entries"][0]["id"], format!("{:032x}", 1));
    // An hour-old claim is forgotten.
    let old = format!(r#"{{"version":1,"entries":[{{"id":"{}","at_ms":1}}]}}"#, "f".repeat(32));
    fs::write(directory.path().join("deliveries.json"), old)?;
    assert!(deliveries.claim(&id('f'))?);
    Ok(())
}

#[test]
fn only_well_formed_ids_are_accepted() {
    for bad in ["", "ABCDEF0123456789ABCDEF0123456789", "../x", &"a".repeat(33)] {
        assert!(DeliveryId::try_from(bad.to_owned()).is_err(), "{bad}");
        assert!(serde_json::from_str::<DeliveryId>(&format!("{bad:?}")).is_err());
    }
}
