// 链上中国宿主只装配公共平台与已安装模块，不静态编入业务。
include!("interface.rs");
fn local_module() -> Option<Arc<dyn modules::model::ModuleBusiness>> { None }
fn main() { run_backend(local_module()); }
