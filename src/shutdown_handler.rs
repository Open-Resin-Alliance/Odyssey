use tokio_util::sync::CancellationToken;

pub struct ShutdownHandler {
    pub cancellation_token: CancellationToken,
}

impl ShutdownHandler {
    pub fn new() -> Self {
        let cancellation_token = CancellationToken::new();
        ShutdownHandler::set_panic_handler(cancellation_token.clone());

        ShutdownHandler { cancellation_token }
    }

    pub async fn until_shutdown(&self) {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => self.cancellation_token.cancel(),
            _ = self.cancellation_token.clone().cancelled_owned() => {},
        }
    }

    fn set_panic_handler(cancellation_token: CancellationToken) {
        let default_panic = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            cancellation_token.cancel();
            default_panic(info);
        }));
    }
}
