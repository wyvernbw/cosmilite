use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::de::DeserializeOwned;
use tokio::task::JoinHandle;

use crate::event::Event;

use crate::socket::AsyncSocket;

type PinFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;
type AsyncUdpSocket = crate::socket::async_udp::Socket;

pub mod async_router {
    use std::ops::Deref;

    use tokio::sync::Mutex;

    use super::*;

    #[derive(Debug, Clone)]
    pub struct State<T>(pub T);

    pub type Connection = Arc<Mutex<AsyncUdpSocket>>;

    pub struct Context<S: Clone> {
        state: State<S>,
        socket: Arc<Mutex<AsyncUdpSocket>>,
    }

    pub trait FromContext<S: Clone>
    where
        Self: Sized,
    {
        fn from_context(ctx: Arc<Context<S>>) -> PinFuture<Option<Self>>;
    }

    impl<S, T> FromContext<S> for Event<T>
    where
        T: DeserializeOwned + Send + 'static,
        S: Clone + Send + Sync + 'static,
    {
        fn from_context(ctx: Arc<Context<S>>) -> PinFuture<Option<Self>> {
            Box::pin(async move {
                let socket = ctx.socket.lock().await;
                socket.recv().await.map(|event| Event(event.0, event.1))
            })
        }
    }

    impl<S, T> FromContext<S> for (Event<T>,)
    where
        T: DeserializeOwned + Send + 'static,
        S: Clone + Send + Sync + 'static,
    {
        fn from_context(ctx: Arc<Context<S>>) -> PinFuture<Option<Self>> {
            Box::pin(async move {
                ctx.socket
                    .lock()
                    .await
                    .recv()
                    .await
                    .map(|event| (Event(event.0, event.1),))
            })
        }
    }

    impl<S> FromContext<S> for State<S>
    where
        S: Clone + Send + Sync + 'static,
    {
        fn from_context(ctx: Arc<Context<S>>) -> PinFuture<Option<Self>> {
            Box::pin(async move { Some(ctx.state.clone()) })
        }
    }

    impl<S> FromContext<S> for (State<S>,)
    where
        S: Clone + Send + Sync + 'static,
    {
        fn from_context(ctx: Arc<Context<S>>) -> PinFuture<Option<Self>> {
            Box::pin(async move { Some((ctx.state.clone(),)) })
        }
    }

    impl<S> FromContext<S> for Connection
    where
        S: Clone + Send + Sync + 'static,
    {
        fn from_context(ctx: Arc<Context<S>>) -> PinFuture<Option<Self>> {
            Box::pin(async move { Some(ctx.socket.clone()) })
        }
    }

    impl<S> FromContext<S> for (Connection,)
    where
        S: Clone + Send + Sync + 'static,
    {
        fn from_context(ctx: Arc<Context<S>>) -> PinFuture<Option<Self>> {
            Box::pin(async move { Some((ctx.socket.clone(),)) })
        }
    }

    pub trait Handler<S: Clone, T> {
        fn call_handler(self: Arc<Self>, ctx: Arc<Context<S>>) -> PinFuture<S>;
    }

    impl<S, T, F, Fut> Handler<S, (T,)> for F
    where
        Fut: Future<Output = S> + Send + Sync + 'static,
        F: Fn(T) -> Fut + Send + Sync + 'static,
        T: FromContext<S> + Send + Sync + 'static,
        S: Clone + Send + Sync + 'static,
    {
        fn call_handler(self: Arc<Self>, ctx: Arc<Context<S>>) -> PinFuture<S> {
            Box::pin(async move {
                let value = T::from_context(ctx.clone()).await;
                match value {
                    Some(value) => self(value).await,
                    None => ctx.state.0.clone(),
                }
            })
        }
    }
    impl<S, T1, T2, F, Fut> Handler<S, (T1, T2)> for F
    where
        Fut: Future<Output = S> + Send + Sync + 'static,
        F: Fn(T1, T2) -> Fut + Send + Sync + 'static,
        T1: FromContext<S> + Send + Sync + 'static,
        T2: FromContext<S> + Send + Sync + 'static,
        S: Clone + Send + Sync + 'static,
    {
        fn call_handler(self: Arc<Self>, ctx: Arc<Context<S>>) -> PinFuture<S> {
            Box::pin(async move {
                let value1 = T1::from_context(ctx.clone()).await;
                let value2 = T2::from_context(ctx.clone()).await;
                match (value1, value2) {
                    (Some(value1), Some(value2)) => self(value1, value2).await,
                    _ => ctx.state.0.clone(),
                }
            })
        }
    }

    impl<S, T1, T2, T3, F, Fut> Handler<S, (T1, T2, T3)> for F
    where
        Fut: Future<Output = S> + Send + Sync + 'static,
        F: Fn(T1, T2, T3) -> Fut + Send + Sync + 'static,
        T1: FromContext<S> + Send + Sync + 'static,
        T2: FromContext<S> + Send + Sync + 'static,
        T3: FromContext<S> + Send + Sync + 'static,
        S: Clone + Send + Sync + 'static,
    {
        fn call_handler(self: Arc<Self>, ctx: Arc<Context<S>>) -> PinFuture<S> {
            Box::pin(async move {
                let value1 = T1::from_context(ctx.clone()).await;
                let value2 = T2::from_context(ctx.clone()).await;
                let value3 = T3::from_context(ctx.clone()).await;
                match (value1, value2, value3) {
                    (Some(value1), Some(value2), Some(value3)) => {
                        self(value1, value2, value3).await
                    }
                    _ => ctx.state.0.clone(),
                }
            })
        }
    }

    pub trait ErasedHandler<S: Clone> {
        fn call_erased_handler(self: Arc<Self>, ctx: Arc<Context<S>>) -> PinFuture<S>;
    }

    impl<S, T> ErasedHandler<S> for Arc<dyn Handler<S, T> + Send + Sync + 'static>
    where
        T: Send + 'static,
        S: Clone + Send + Sync + 'static,
    {
        fn call_erased_handler(self: Arc<Self>, ctx: Arc<Context<S>>) -> PinFuture<S> {
            Box::pin(async move {
                let inner = self.deref().clone();
                inner.call_handler(ctx).await
            })
        }
    }

    pub struct Router<S: Clone> {
        handlers: Vec<Arc<dyn ErasedHandler<S> + Send + Sync + 'static>>,
        ctx: Arc<Context<S>>,
    }

    impl Router<()> {
        pub fn new(socket: AsyncUdpSocket) -> Self {
            Self {
                handlers: Vec::new(),
                ctx: Arc::new(Context {
                    state: State(()),
                    socket: Arc::new(Mutex::new(socket)),
                }),
            }
        }
        pub fn with_state<S: Clone>(self, state: S) -> Router<S> {
            Router {
                handlers: Vec::new(),
                ctx: Arc::new(Context {
                    state: State(state),
                    socket: self.ctx.socket.clone(),
                }),
            }
        }
    }

    impl<S> Router<S>
    where
        S: Clone + Send + Sync + 'static,
    {
        pub fn route<T, H>(mut self, handler: H) -> Self
        where
            T: Send + Sync + 'static,
            H: Handler<S, T> + Send + Sync + 'static,
        {
            let handler = Arc::new(handler) as Arc<dyn Handler<S, T> + Send + Sync + 'static>;
            let handler = Arc::new(handler) as Arc<dyn ErasedHandler<S> + Send + Sync + 'static>;
            self.handlers.push(handler);
            self
        }
        pub fn run(self) -> JoinHandle<()> {
            let ctx = self.ctx;
            tokio::spawn(async move {
                loop {
                    let ctx = ctx.clone();
                    let handlers = self.handlers.clone();
                    tokio::spawn(async move {
                        let state = ctx.state.clone();
                        for handler in handlers {
                            handler.call_erased_handler(ctx.clone()).await;
                        }
                    });
                }
            })
        }
    }
}
