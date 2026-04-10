mod protocol;
mod router;
mod tools;
mod transport;

fn main() {
    let registry = tools::register_all();
    let router = router::Router::new(registry);
    transport::run_stdio(router);
}
