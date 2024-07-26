use std::{
    future::poll_fn,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};

fn main() {
    let body = async {
        let state = Arc::new(Mutex::new(State::new()));

        let mut subscriber = Subscriber::new(Arc::clone(&state), 1);
        let handle = tokio::spawn(async move {
            subscriber.wait().await;
            subscriber.wait().await;
        });

        tokio::spawn(async move {
            state.lock().unwrap().set_version(2);
            state.lock().unwrap().set_version(0);
        });

        handle.await.unwrap();
    };

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed building the Runtime")
        .block_on(body);
}

pub(crate) struct Subscriber {
    state: Arc<Mutex<State>>,
    observed_version: u64,
    waker_key: Option<usize>,
}

impl Subscriber {
    pub(crate) fn new(state: Arc<Mutex<State>>, version: u64) -> Self {
        Self {
            state,
            observed_version: version,
            waker_key: None,
        }
    }

    pub(crate) async fn wait(&mut self) {
        poll_fn(|cx| {
            self.state
                .lock()
                .unwrap()
                .poll_update(&mut self.observed_version, &mut self.waker_key, cx)
                .map(|_| ())
        })
        .await;
    }
}

struct State {
    version: u64,
    wakers: Vec<Waker>,
}

impl State {
    pub(crate) fn new() -> Self {
        Self {
            version: 1,
            wakers: Vec::new(),
        }
    }

    pub(crate) fn poll_update(
        &mut self,
        observed_version: &mut u64,
        waker_key: &mut Option<usize>,
        cx: &Context<'_>,
    ) -> Poll<Option<()>> {
        if self.version == 0 {
            *waker_key = None;
            Poll::Ready(None)
        } else if *observed_version < self.version {
            *waker_key = None;
            *observed_version = self.version;
            Poll::Ready(Some(()))
        } else {
            self.wakers.push(cx.waker().clone());
            *waker_key = Some(self.wakers.len());
            Poll::Pending
        }
    }

    pub(crate) fn set_version(&mut self, version: u64) {
        self.version = version;
        for waker in self.wakers.drain(..) {
            waker.wake();
        }
    }
}
