use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub(crate) struct AddressScopeQuery {
    pub(crate) province_code: String,
    pub(crate) city_code: String,
    pub(crate) town_code: String,
    pub(crate) address_name_code: Option<String>,
    pub(crate) cursor: Option<usize>,
    pub(crate) page_size: Option<usize>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AddressNameRow {
    pub(crate) province_code: String,
    pub(crate) city_code: String,
    pub(crate) town_code: String,
    pub(crate) address_name_code: String,
    pub(crate) address_name: String,
    pub(crate) address_count: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct AddressRow {
    pub(crate) province_code: String,
    pub(crate) city_code: String,
    pub(crate) town_code: String,
    pub(crate) address_name_code: String,
    pub(crate) address_name: String,
    pub(crate) address_local_no: String,
    pub(crate) address_detail: String,
    pub(crate) sort_order: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct AddressPage<T: Serialize> {
    pub(crate) items: Vec<T>,
    pub(crate) page_size: usize,
    pub(crate) next_cursor: Option<usize>,
    pub(crate) has_more: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AddressChainAction {
    SetCatalogVersion,
    SetAddressName,
    RemoveAddressName,
    SetAddress,
    RemoveAddress,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AddressChainCallInput {
    pub(crate) actor_role_code: String,
    pub(crate) action: AddressChainAction,
    #[serde(default)]
    pub(crate) catalog_version: Option<String>,
    #[serde(default)]
    pub(crate) catalog_hash: Option<String>,
    #[serde(default)]
    pub(crate) province_code: Option<String>,
    #[serde(default)]
    pub(crate) city_code: Option<String>,
    #[serde(default)]
    pub(crate) town_code: Option<String>,
    #[serde(default)]
    pub(crate) address_name_code: Option<String>,
    #[serde(default)]
    pub(crate) address_name: Option<String>,
    #[serde(default)]
    pub(crate) address_local_no: Option<String>,
    #[serde(default)]
    pub(crate) address_detail: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AddressChainCallOutput {
    pub(crate) action: u16,
    pub(crate) pallet_index: u8,
    pub(crate) call_index: u8,
    pub(crate) call_data_hex: String,
    pub(crate) review_title: &'static str,
}
