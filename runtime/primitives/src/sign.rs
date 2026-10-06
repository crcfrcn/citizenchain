//! 全仓签名消息常量与唯一构造入口。
//!
//! 链上交易默认使用 Substrate extrinsic 签名;只有第三方背书、链下支付、链下 challenge
//! 或跨上下文离线证明才使用本模块 op_tag。哈希域统一走 [`signing_message`],二进制前缀域
//! 只使用 `GMB || op_tag` 前缀。Dart/TS 镜像必须与本文件和金标向量保持一致。

use crate::core_const::GMB; // 域分隔符(地址派生 + 签名共用),单源在 core_const
use sp_crypto_hashing::blake2_256;
use sp_std::vec::Vec;

// QR_V1 场景/动作编号。链交易动作码统一由 `qr_chain_action` 生成。

/// QR_V1 签名请求场景:生成方展示二维码,扫码方识别并签名。
pub const QR_KIND_SIGN_REQUEST: u8 = 1;
/// QR_V1 签名响应场景:扫码方展示签名结果,生成方扫码验签。
pub const QR_KIND_SIGN_RESPONSE: u8 = 2;
/// QR_V1 用户联系人固定码。
pub const QR_KIND_USER_CONTACT: u8 = 3;
/// QR_V1 用户转账固定码。
pub const QR_KIND_USER_TRANSFER: u8 = 4;

/// QR_V1 登录签名动作。
pub const QR_ACTION_LOGIN: u16 = 1;
/// QR_V1 公民档案上链确认签名动作。
pub const QR_ACTION_CITIZEN_IDENTITY: u16 = 2;
/// QR_V1 链上中国平台管理员治理/Passkey 更新签名动作。
pub const QR_ACTION_ONCHINA_ADMIN: u16 = 3;
/// QR_V1 管理员激活二进制原始签名动作。
pub const QR_ACTION_ACTIVATE_ADMIN: u16 = 5;
/// QR_V1 清算行管理员解密二进制原始签名动作。
pub const QR_ACTION_DECRYPT_ADMIN: u16 = 6;
/// QR_V1 runtime 升级 32 字节哈希直签动作。
pub const QR_ACTION_RUNTIME_UPGRADE_HASH: u16 = 7;
/// QR_V1 广场账户动作（订阅/取消/…）链下签名动作，映射 op_tag OP_SIGN_SQUARE_ACTION(0x1D)。
pub const QR_ACTION_SQUARE_ACCOUNT: u16 = 9;

/// 链交易二维码动作码:高 8 位是 pallet index,低 8 位是 call index。
pub const fn qr_chain_action(pallet_index: u8, call_index: u8) -> u16 {
    ((pallet_index as u16) << 8) | call_index as u16
}

// 签名 op_tag 单一权威源:
// - 0x10-0x13/0x14-0x17:哈希域,走 `signing_message`,进入 `SIGN_OP_TAGS`。
// - 0x18/0x19:二进制前缀域,只签原始 payload,不进入 `SIGN_OP_TAGS`。
// - 0x1C:同一MLS公钥登记；0x1D:广场账户敏感动作。均由当前绑定账户授权，
//   链下服务验证；签名摘要仍走统一signing_message。
// - 0x1E:GRANDPA 验证密钥更换证明哈希域，由旧、新 ed25519 私钥签同一摘要。
// - 0x1F:注册局代办换绑哈希域,与首次占号(0x12)域分离。
// - 0x20:OnChina管理员治理哈希域；账户派生段与签名段分离。
// - 0x21:CitizenApp 本机默认账户切换哈希域。只证明原默认账户授权完整目标顺序,
//   不包含 CID/binding revision,不进入 pallet、Storage、Extrinsic 或换绑流程。
// - 0x23:CitizenApp 钱包账户签名模式确认哈希域。只用于本机验证热钱包私钥确实
//   控制目标 AccountId 后写入 Hot；不提交链、不修改账户控制权。
// - 0x24:本机生产发布授权哈希域。冷钱包签署精确 Release、产物摘要、
//   回滚锚点和短时随机挑战；不提交链，也不向发布平台发送钱包信息。
//   新增签名 op_tag 一律往上顺延,禁止回填 0x00-0x0F 或复用已删域的旧值。

/// 公民档案上链确认。
pub const OP_SIGN_CITIZEN_IDENTITY: u8 = 0x10;
/// 匿名 CID 自助换绑：旧绑定账户签署含创世哈希、旧/新账户、binding revision 和
/// Unix 秒过期时间的 `CidRebindAuthorization`（哈希域）。
pub const OP_SIGN_CID_REBIND: u8 = 0x11;
/// 注册局首次占号(占即绑)：新账户签署含创世哈希、固定 revision=0 和 Unix 秒过期时间
/// 的 `CidOccupyAuthorization`，证明账户受控（哈希域）。
pub const OP_SIGN_CID_OCCUPY: u8 = 0x12;
/// CID 机构登记(历史 op_tag,已无独立凭证构造入口;仅作为四端 `SIGN_OP_TAGS` 金标
/// 注册表成员保留,删除会扰动四端字节契约与金标向量)。
pub const OP_SIGN_INST: u8 = 0x13;
/// CID 机构/账户注销凭证(历史 op_tag)。注册局审批凭证已删除,机构自定义账户关闭改为
/// 机构在册管理员直接冷签 `propose_close`(不含凭证),链端在 origin 处以 `is_institution_admin`
/// 鉴权。本常量已无 message 构造入口,仅作为四端 `SIGN_OP_TAGS` 金标注册表成员保留,
/// 删除会扰动四端字节契约与金标向量。
pub const OP_SIGN_DEREGISTER: u8 = 0x14;

/// L3 支付。
pub const OP_SIGN_L3_PAY: u8 = 0x15;
/// 链下批次结算。
pub const OP_SIGN_OFFCHAIN_BATCH: u8 = 0x16;
/// L2 确认。
pub const OP_SIGN_L2_ACK: u8 = 0x17;

/// 管理员激活二进制前缀域;不走 `signing_message`。
pub const OP_SIGN_ACTIVATE_ADMIN: u8 = 0x18;
/// 解密授权二进制前缀域;不走 `signing_message`。
pub const OP_SIGN_DECRYPT: u8 = 0x19;

/// 同一MLS公钥登记；由当前CID绑定的sr25519账户授权，链下服务验签。
pub const OP_SIGN_MLS_DEVICE_BIND: u8 = 0x1C;
/// 广场 BFF 账户敏感动作:注销/退订(链下 Worker 验签,sr25519 主钥签)。
pub const OP_SIGN_SQUARE_ACTION: u8 = 0x1D;
/// GRANDPA 验证密钥正常更换与紧急恢复的持钥证明。
pub const OP_SIGN_GRANDPA_KEY_CHANGE: u8 = 0x1E;
/// 注册局代匿名或实名 CID 换绑：新账户签署 `CidRebindAuthorization` 控制证明。
/// 与首次占号域分离，且载荷自带创世哈希、当前 revision 与过期时间。
pub const OP_SIGN_CID_ADMIN_REBIND: u8 = 0x1F;
/// 链上中国平台管理员治理动作(增删管理员、机构创建/更新、账户增删、文档、
/// Passkey 更新、电子护照绑定确认等)的冷签授权。
///
/// 此前这条链路对**裸 JSON 文本直签**:无 `GMB` 前缀、无 op_tag、不进本表,
/// 域分离只靠 JSON 里一个 `domain` 字符串字段 —— 结构性违反「签名唯一入口」死规则,
/// 新增其它 JSON 直签域时没有任何编译期或金标机制能挡住结构碰撞。已收敛到本域。
pub const OP_SIGN_ONCHINA_ADMIN: u8 = 0x20;
/// CitizenApp 本机切换默认账户：变化前的原默认账户签署完整目标账户顺序。
/// 该证明只在移动端本机验签，不提交链，也不得复用 CID 换绑域。
pub const OP_SIGN_SWITCH_DEFAULT_ACCOUNT: u8 = 0x21;
/// CitizenApp 钱包账户签名模式确认：本机私钥签署创世哈希、目标 AccountId、
/// `hot` 模式与一次性挑战。只用于本机重标验证，不提交链。
pub const OP_SIGN_WALLET_MODE: u8 = 0x23;
/// 本机生产发布授权：只在本机校验，且必须与 Touch ID 同时通过。
pub const OP_SIGN_PUBLISH: u8 = 0x24;

/// 二进制前缀域(0x18/0x19)统一前缀长度:`GMB`(3B) + op_tag(1B) = 4 字节。
pub const BINARY_PREFIX_LEN: usize = 4;
/// 管理员激活载荷中的 CID 固定槽长度，与链上 CID 上限一致。
pub const ACTIVATE_ADMIN_CID_LEN: usize = crate::core_const::CID_NUMBER_MAX_BYTES as usize;
/// 管理员激活原始签名载荷固定长度。
pub const ACTIVATE_ADMIN_PAYLOAD_LEN: usize =
    BINARY_PREFIX_LEN + ACTIVATE_ADMIN_CID_LEN + 4 + 1 + 32 + 8 + 16;
/// 管理员解密载荷中的 CID 固定槽长度，与链上 CID 上限一致。
pub const DECRYPT_ADMIN_CID_LEN: usize = crate::core_const::CID_NUMBER_MAX_BYTES as usize;
/// 管理员解密原始签名载荷固定长度。
pub const DECRYPT_ADMIN_PAYLOAD_LEN: usize =
    BINARY_PREFIX_LEN + DECRYPT_ADMIN_CID_LEN + 32 + 8 + 16;

/// 构造二进制前缀域的 4 字节前缀 `GMB || op_tag`(0x18/0x19 用)。
pub fn binary_domain_prefix(op_tag: u8) -> [u8; BINARY_PREFIX_LEN] {
    let mut prefix = [0u8; BINARY_PREFIX_LEN];
    prefix[..GMB.len()].copy_from_slice(GMB);
    prefix[GMB.len()] = op_tag;
    prefix
}

/// 构造机构管理员本地激活原始签名载荷。
///
/// 布局固定为 `GMB || 0x18 || cid_number(32B,右补零) || institution_code(4B)
/// || kind(1B) || signer_public_key(32B) || timestamp_le(8B) || nonce(16B)`。
/// CID 是机构唯一主键，协议账户不参与本地管理员身份绑定。
pub fn activate_admin_payload(
    cid_number: &[u8],
    institution_code: &[u8; 4],
    kind: u8,
    signer_public_key: &[u8; 32],
    timestamp: u64,
    nonce: &[u8; 16],
) -> Option<Vec<u8>> {
    if cid_number.is_empty() || cid_number.len() > ACTIVATE_ADMIN_CID_LEN {
        return None;
    }
    let mut payload = Vec::with_capacity(ACTIVATE_ADMIN_PAYLOAD_LEN);
    payload.extend_from_slice(&binary_domain_prefix(OP_SIGN_ACTIVATE_ADMIN));
    payload.extend_from_slice(cid_number);
    payload.resize(BINARY_PREFIX_LEN + ACTIVATE_ADMIN_CID_LEN, 0);
    payload.extend_from_slice(institution_code);
    payload.push(kind);
    payload.extend_from_slice(signer_public_key);
    payload.extend_from_slice(&timestamp.to_le_bytes());
    payload.extend_from_slice(nonce);
    Some(payload)
}

/// 构造清算行管理员本地解密原始签名载荷。
///
/// 布局固定为 `GMB || 0x19 || cid_number(32B,右补零) || signer_public_key(32B)
/// || timestamp_le(8B) || nonce(16B)`。构造方、解析方和冷钱包必须共同使用
/// 本函数及同组长度常量，禁止另设 48B CID 槽或手工第二布局。
pub fn decrypt_admin_payload(
    cid_number: &[u8],
    signer_public_key: &[u8; 32],
    timestamp: u64,
    nonce: &[u8; 16],
) -> Option<Vec<u8>> {
    if cid_number.is_empty() || cid_number.len() > DECRYPT_ADMIN_CID_LEN {
        return None;
    }
    let mut payload = Vec::with_capacity(DECRYPT_ADMIN_PAYLOAD_LEN);
    payload.extend_from_slice(&binary_domain_prefix(OP_SIGN_DECRYPT));
    payload.extend_from_slice(cid_number);
    payload.resize(BINARY_PREFIX_LEN + DECRYPT_ADMIN_CID_LEN, 0);
    payload.extend_from_slice(signer_public_key);
    payload.extend_from_slice(&timestamp.to_le_bytes());
    payload.extend_from_slice(nonce);
    Some(payload)
}

/// 全部哈希域签名 op_tag。新增哈希域 op_tag 必须同步追加并刷新金标。
pub const SIGN_OP_TAGS: [u8; 16] = [
    OP_SIGN_CITIZEN_IDENTITY,
    OP_SIGN_CID_REBIND,
    OP_SIGN_CID_OCCUPY,
    OP_SIGN_INST,
    OP_SIGN_DEREGISTER,
    OP_SIGN_L3_PAY,
    OP_SIGN_OFFCHAIN_BATCH,
    OP_SIGN_L2_ACK,
    OP_SIGN_MLS_DEVICE_BIND,
    OP_SIGN_SQUARE_ACTION,
    OP_SIGN_GRANDPA_KEY_CHANGE,
    OP_SIGN_CID_ADMIN_REBIND,
    OP_SIGN_ONCHINA_ADMIN,
    OP_SIGN_SWITCH_DEFAULT_ACCOUNT,
    OP_SIGN_WALLET_MODE,
    OP_SIGN_PUBLISH,
];

/// 构造哈希域签名消息:`BLAKE2-256(GMB || op_tag || scale_payload)`。
pub fn signing_message(op_tag: u8, scale_payload: &[u8]) -> [u8; 32] {
    let mut data = Vec::with_capacity(GMB.len() + 1 + scale_payload.len());
    data.extend_from_slice(GMB);
    data.push(op_tag);
    data.extend_from_slice(scale_payload);
    blake2_256(&data)
}

// 机构登记/创建/治理/账户关闭均已收敛为「发起管理员账户直接冷签一笔普通 extrinsic」,由 runtime
// 在 origin 处以 `is_institution_admin` 鉴权,不再有任何独立凭证签名消息。原
// `institution_account_close_message`(注册局审批凭证)连同 OnChina 平台签名钥已整体删除。

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activate_admin_payload_uses_the_shared_32_byte_cid_slot() {
        let cid_number = b"LN001-NRC0G-944805165-2026";
        let institution_code = *b"NRC\0";
        let signer_public_key = [0x22; 32];
        let nonce = [0u8; 16];
        let payload = activate_admin_payload(
            cid_number,
            &institution_code,
            0,
            &signer_public_key,
            1_700_000_000,
            &nonce,
        )
        .expect("valid CID should build an activation payload");

        assert_eq!(payload.len(), ACTIVATE_ADMIN_PAYLOAD_LEN);
        assert_eq!(&payload[..BINARY_PREFIX_LEN], b"GMB\x18");
        assert_eq!(
            &payload[BINARY_PREFIX_LEN..BINARY_PREFIX_LEN + cid_number.len()],
            cid_number
        );
        assert!(payload
            [BINARY_PREFIX_LEN + cid_number.len()..BINARY_PREFIX_LEN + ACTIVATE_ADMIN_CID_LEN]
            .iter()
            .all(|byte| *byte == 0));
    }

    #[test]
    fn decrypt_admin_payload_uses_the_same_cid_limit_and_rejects_invalid_cids() {
        let cid_number = b"AH001-SCB0V-123456789-2026";
        let signer_public_key = [0x33; 32];
        let nonce = [0u8; 16];
        let payload = decrypt_admin_payload(cid_number, &signer_public_key, 1_700_000_000, &nonce)
            .expect("valid CID should build a decrypt payload");

        assert_eq!(payload.len(), DECRYPT_ADMIN_PAYLOAD_LEN);
        assert_eq!(&payload[..BINARY_PREFIX_LEN], b"GMB\x19");
        assert_eq!(
            &payload[BINARY_PREFIX_LEN..BINARY_PREFIX_LEN + cid_number.len()],
            cid_number
        );
        assert!(payload
            [BINARY_PREFIX_LEN + cid_number.len()..BINARY_PREFIX_LEN + DECRYPT_ADMIN_CID_LEN]
            .iter()
            .all(|byte| *byte == 0));
        assert!(decrypt_admin_payload(&[], &signer_public_key, 0, &nonce).is_none());
        assert!(decrypt_admin_payload(
            &[b'X'; DECRYPT_ADMIN_CID_LEN + 1],
            &signer_public_key,
            0,
            &nonce,
        )
        .is_none());
    }
}
