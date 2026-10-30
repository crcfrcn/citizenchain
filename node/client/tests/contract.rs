use crate::query::system_account_key;

#[test]
fn system_account_key_has_correct_length() {
    let account_id = "0xa5423e483bba281da84b99620a670718d5a7eceb5ae720f7d492e8b5c2570d84";
    let key = system_account_key(account_id).unwrap();
    // 0x 前缀 + (16+16+16+32)*2 hex 字符 = 2 + 160 = 162
    assert_eq!(key.len(), 162);
}
