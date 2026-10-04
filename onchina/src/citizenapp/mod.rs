//! CitizenApp BFF(Backend-For-Frontend)—— 公民端匿名只读接口归口。
//!
//! 本目录只承载 CitizenApp 公民端**匿名只读**接口的薄 handler + DTO;
//! 领域逻辑一律留在各领域模块(公权机构目录查询留 `gov`/`Db`,本层只调不复制)。
//! 安全红线:目录整层即匿名可读边界,**只透白名单公开字段**,严禁带
//! creator_account_id 等管理员/PII 字段(见 public_institution::PublicInstitutionRow)。
//!
//! 路由挂在 main.rs 的 `app_routes`(非 admin、`/api/app/` 命名空间)。

pub(crate) mod public_institution;
