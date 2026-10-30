use super::*;

#[test]
fn primitive_institution_records_registry_and_serialized_overview_agree() {
    let overview = governance_overview();
    assert_eq!(
        overview.national_councils.len(),
        usize::from(!CHINA_CB.is_empty())
    );
    assert_eq!(
        overview.provincial_councils.len(),
        CHINA_CB.len().saturating_sub(1)
    );
    assert_eq!(overview.provincial_banks.len(), CHINA_CH.len());
    for record in CHINA_CB.iter() {
        let found = find_institution(record.cid_number).unwrap();
        assert_eq!(found.cid_full_name(), record.cid_full_name);
        assert_eq!(found.cid_short_name(), record.cid_short_name);
        let json = serde_json::to_value(found.to_list_item()).unwrap();
        assert_eq!(json["cidNumber"], record.cid_number);
        assert_eq!(
            json["mainAccountId"],
            format!("0x{}", hex::encode(record.main_account))
        );
    }
    for record in CHINA_CH.iter() {
        assert_eq!(
            find_institution(record.cid_number).unwrap().cid_full_name(),
            record.cid_full_name
        );
    }
    assert!(find_institution("unregistered-test-cid").is_none());
}
