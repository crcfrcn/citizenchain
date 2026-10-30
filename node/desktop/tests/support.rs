//! 原隔离页面测试注入，仅测试构建开放。
use super::*;
impl RpcTls {
    /// 隔离视图只供测试会话注入，正式进程没有页面替换环境变量或磁盘读取入口。
    pub fn with_test_page(mut self, page: Vec<u8>) -> Result<Self, RpcError> {
        let html = std::str::from_utf8(&page)?;
        if page.len() > 128 * 1024
            || !html.starts_with("<!doctype html>")
            || !html.ends_with("</html>\n")
        {
            return Err("隔离接入页必须是有界的完整 HTML".into());
        }
        self.page = Arc::from(page);
        Ok(self)
    }
}
