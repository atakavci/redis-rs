use std::time::Duration;

#[cfg(feature = "aio")]
use crate::aio::{AsyncPushSender, DefaultAsyncDNSResolver};
#[cfg(feature = "aio")]
use crate::io::{tcp::TcpSettings, AsyncDNSResolver};
use crate::{
    connection::{connect, Connection, ConnectionInfo, ConnectionLike, IntoConnectionInfo},
    types::{RedisResult, Value},
};
#[cfg(feature = "aio")]
use std::pin::Pin;

#[cfg(feature = "tls-rustls")]
use crate::tls::{inner_build_with_tls, TlsCertificates};

#[cfg(feature = "cache-aio")]
use crate::caching::CacheConfig;
#[cfg(all(
    feature = "cache-aio",
    any(feature = "connection-manager", feature = "cluster-async")
))]
use crate::caching::CacheManager;

/// The client type.
#[derive(Debug, Clone)]
pub struct Client {
    pub(crate) connection_info: ConnectionInfo,
}

/// The client acts as connector to the redis server.  By itself it does not
/// do much other than providing a convenient way to fetch a connection from
/// it.  In the future the plan is to provide a connection pool in the client.
///
/// When opening a client a URL in the following format should be used:
///
/// ```plain
/// redis://host:port/db
/// ```
///
/// Example usage::
///
/// ```rust,no_run
/// let client = redis::Client::open("redis://127.0.0.1/").unwrap();
/// let con = client.get_connection().unwrap();
/// ```
impl Client {
    /// Connects to a redis server and returns a client.  This does not
    /// actually open a connection yet but it does perform some basic
    /// checks on the URL that might make the operation fail.
    pub fn open<T: IntoConnectionInfo>(params: T) -> RedisResult<Client> {
        Ok(Client {
            connection_info: params.into_connection_info()?,
        })
    }

    /// Instructs the client to actually connect to redis and returns a
    /// connection object.  The connection object can be used to send
    /// commands to the server.  This can fail with a variety of errors
    /// (like unreachable host) so it's important that you handle those
    /// errors.
    pub fn get_connection(&self) -> RedisResult<Connection> {
        connect(&self.connection_info, None)
    }

    /// Instructs the client to actually connect to redis with specified
    /// timeout and returns a connection object.  The connection object
    /// can be used to send commands to the server.  This can fail with
    /// a variety of errors (like unreachable host) so it's important
    /// that you handle those errors.
    pub fn get_connection_with_timeout(&self, timeout: Duration) -> RedisResult<Connection> {
        connect(&self.connection_info, Some(timeout))
    }

    /// Returns a reference of client connection info object.
    pub fn get_connection_info(&self) -> &ConnectionInfo {
        &self.connection_info
    }

    /// Constructs a new `Client` with parameters necessary to create a TLS connection.
    ///
    /// - `conn_info` - URL using the `rediss://` scheme.
    /// - `tls_certs` - `TlsCertificates` structure containing:
    ///     - `client_tls` - Optional `ClientTlsConfig` containing byte streams for
    ///         - `client_cert` - client's byte stream containing client certificate in PEM format
    ///         - `client_key` - client's byte stream containing private key in PEM format
    ///     - `root_cert` - Optional byte stream yielding PEM formatted file for root certificates.
    ///
    /// If `ClientTlsConfig` ( cert+key pair ) is not provided, then client-side authentication is not enabled.
    /// If `root_cert` is not provided, then system root certificates are used instead.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::{fs::File, io::{BufReader, Read}};
    ///
    /// use redis::{Client, AsyncCommands as _, TlsCertificates, ClientTlsConfig};
    ///
    /// async fn do_redis_code(
    ///     url: &str,
    ///     root_cert_file: &str,
    ///     cert_file: &str,
    ///     key_file: &str
    /// ) -> redis::RedisResult<()> {
    ///     let root_cert_file = File::open(root_cert_file).expect("cannot open private cert file");
    ///     let mut root_cert_vec = Vec::new();
    ///     BufReader::new(root_cert_file)
    ///         .read_to_end(&mut root_cert_vec)
    ///         .expect("Unable to read ROOT cert file");
    ///
    ///     let cert_file = File::open(cert_file).expect("cannot open private cert file");
    ///     let mut client_cert_vec = Vec::new();
    ///     BufReader::new(cert_file)
    ///         .read_to_end(&mut client_cert_vec)
    ///         .expect("Unable to read client cert file");
    ///
    ///     let key_file = File::open(key_file).expect("cannot open private key file");
    ///     let mut client_key_vec = Vec::new();
    ///     BufReader::new(key_file)
    ///         .read_to_end(&mut client_key_vec)
    ///         .expect("Unable to read client key file");
    ///
    ///     let client = Client::build_with_tls(
    ///         url,
    ///         TlsCertificates {
    ///             client_tls: Some(ClientTlsConfig{
    ///                 client_cert: client_cert_vec,
    ///                 client_key: client_key_vec,
    ///             }),
    ///             root_cert: Some(root_cert_vec),
    ///         }
    ///     )
    ///     .expect("Unable to build client");
    ///
    ///     let connection_info = client.get_connection_info();
    ///
    ///     println!(">>> connection info: {connection_info:?}");
    ///
    ///     let mut con = client.get_multiplexed_async_connection().await?;
    ///
    ///     con.set("key1", b"foo").await?;
    ///
    ///     redis::cmd("SET")
    ///         .arg(&["key2", "bar"])
    ///         .exec_async(&mut con)
    ///         .await?;
    ///
    ///     let result = redis::cmd("MGET")
    ///         .arg(&["key1", "key2"])
    ///         .query_async(&mut con)
    ///         .await;
    ///     assert_eq!(result, Ok(("foo".to_string(), b"bar".to_vec())));
    ///     println!("Result from MGET: {result:?}");
    ///
    ///     Ok(())
    /// }
    /// ```
    #[cfg(feature = "tls-rustls")]
    pub fn build_with_tls<C: IntoConnectionInfo>(
        conn_info: C,
        tls_certs: TlsCertificates,
    ) -> RedisResult<Client> {
        let connection_info = conn_info.into_connection_info()?;

        inner_build_with_tls(connection_info, &tls_certs)
    }
}

#[cfg(feature = "cache-aio")]
#[derive(Clone)]
pub(crate) enum Cache {
    Config(CacheConfig),
    #[cfg(any(feature = "connection-manager", feature = "cluster-async"))]
    Manager(CacheManager),
}

/// Options for creation of async connection
#[cfg(feature = "aio")]
#[derive(Clone, Default)]
pub struct AsyncConnectionConfig {
    /// Maximum time to wait for a response from the server
    pub(crate) response_timeout: Option<std::time::Duration>,
    /// Maximum time to wait for a connection to be established
    pub(crate) connection_timeout: Option<std::time::Duration>,
    pub(crate) push_sender: Option<std::sync::Arc<dyn AsyncPushSender>>,
    #[cfg(feature = "cache-aio")]
    pub(crate) cache: Option<Cache>,
    pub(crate) tcp_settings: TcpSettings,
    pub(crate) dns_resolver: Option<std::sync::Arc<dyn AsyncDNSResolver>>,
}

#[cfg(feature = "aio")]
impl AsyncConnectionConfig {
    /// Creates a new instance of the options with nothing set
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the connection timeout
    pub fn set_connection_timeout(mut self, connection_timeout: std::time::Duration) -> Self {
        self.connection_timeout = Some(connection_timeout);
        self
    }

    /// Sets the response timeout
    pub fn set_response_timeout(mut self, response_timeout: std::time::Duration) -> Self {
        self.response_timeout = Some(response_timeout);
        self
    }

    /// Sets sender sender for push values.
    ///
    /// The sender can be a channel, or an arbitrary function that handles [crate::PushInfo] values.
    /// This will fail client creation if the connection isn't configured for RESP3 communications via the [crate::RedisConnectionInfo::protocol] field.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use redis::AsyncConnectionConfig;
    /// let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    /// let config = AsyncConnectionConfig::new().set_push_sender(tx);
    /// ```
    ///
    /// ```rust
    /// # use std::sync::{Mutex, Arc};
    /// # use redis::AsyncConnectionConfig;
    /// let messages = Arc::new(Mutex::new(Vec::new()));
    /// let config = AsyncConnectionConfig::new().set_push_sender(move |msg|{
    ///     let Ok(mut messages) = messages.lock() else {
    ///         return Err(redis::aio::SendError);
    ///     };
    ///     messages.push(msg);
    ///     Ok(())
    /// });
    /// ```
    pub fn set_push_sender(self, sender: impl AsyncPushSender) -> Self {
        self.set_push_sender_internal(std::sync::Arc::new(sender))
    }

    pub(crate) fn set_push_sender_internal(
        mut self,
        sender: std::sync::Arc<dyn AsyncPushSender>,
    ) -> Self {
        self.push_sender = Some(sender);
        self
    }

    /// Sets cache config for MultiplexedConnection, check CacheConfig for more details.
    #[cfg(feature = "cache-aio")]
    pub fn set_cache_config(mut self, cache_config: CacheConfig) -> Self {
        self.cache = Some(Cache::Config(cache_config));
        self
    }

    #[cfg(all(
        feature = "cache-aio",
        any(feature = "connection-manager", feature = "cluster-async")
    ))]
    pub(crate) fn set_cache_manager(mut self, cache_manager: CacheManager) -> Self {
        self.cache = Some(Cache::Manager(cache_manager));
        self
    }

    /// Set the behavior of the underlying TCP connection.
    pub fn set_tcp_settings(self, tcp_settings: crate::io::tcp::TcpSettings) -> Self {
        Self {
            tcp_settings,
            ..self
        }
    }

    /// Set the DNS resolver for the underlying TCP connection.
    ///
    /// The parameter resolver must implement the [`crate::io::AsyncDNSResolver`] trait.
    pub fn set_dns_resolver(self, dns_resolver: impl AsyncDNSResolver) -> Self {
        self.set_dns_resolver_internal(std::sync::Arc::new(dns_resolver))
    }

    pub(super) fn set_dns_resolver_internal(
        mut self,
        dns_resolver: std::sync::Arc<dyn AsyncDNSResolver>,
    ) -> Self {
        self.dns_resolver = Some(dns_resolver);
        self
    }
}

/// To enable async support you need to chose one of the supported runtimes and active its
/// corresponding feature: `tokio-comp` or `async-std-comp`
#[cfg(feature = "aio")]
#[cfg_attr(docsrs, doc(cfg(feature = "aio")))]
impl Client {
    /// Returns an async connection from the client.
    #[cfg(feature = "aio")]
    #[cfg_attr(docsrs, doc(cfg(feature = "aio")))]
    pub async fn get_multiplexed_async_connection(
        &self,
    ) -> RedisResult<crate::aio::MultiplexedConnection> {
        self.get_multiplexed_async_connection_with_config(&AsyncConnectionConfig::new())
            .await
    }

    /// Returns an async connection from the client.
    #[cfg(feature = "aio")]
    #[cfg_attr(docsrs, doc(cfg(feature = "aio")))]
    #[deprecated(note = "Use `get_multiplexed_async_connection_with_config` instead")]
    pub async fn get_multiplexed_async_connection_with_timeouts(
        &self,
        response_timeout: std::time::Duration,
        connection_timeout: std::time::Duration,
    ) -> RedisResult<crate::aio::MultiplexedConnection> {
        self.get_multiplexed_async_connection_with_config(
            &AsyncConnectionConfig::new()
                .set_connection_timeout(connection_timeout)
                .set_response_timeout(response_timeout),
        )
        .await
    }

    /// Returns an async connection from the client.
    #[cfg(feature = "aio")]
    #[cfg_attr(docsrs, doc(cfg(feature = "aio")))]
    pub async fn get_multiplexed_async_connection_with_config(
        &self,
        config: &AsyncConnectionConfig,
    ) -> RedisResult<crate::aio::MultiplexedConnection> {
        match Runtime::locate() {
            #[cfg(feature = "tokio-comp")]
            rt @ Runtime::Tokio => self
                .get_multiplexed_async_connection_inner_with_timeout::<crate::aio::tokio::Tokio>(
                    config, rt,
                )
                .await,

            #[cfg(feature = "async-std-comp")]
            rt @ Runtime::AsyncStd => self.get_multiplexed_async_connection_inner_with_timeout::<
                crate::aio::async_std::AsyncStd,
            >(config, rt)
            .await,

            #[cfg(feature = "smol-comp")]
            rt @ Runtime::Smol => self.get_multiplexed_async_connection_inner_with_timeout::<
                crate::aio::smol::Smol,
            >(config, rt)
            .await,
        }
    }

    /// Returns an async multiplexed connection from the client.
    ///
    /// A multiplexed connection can be cloned, allowing requests to be sent concurrently
    /// on the same underlying connection (tcp/unix socket).
    #[cfg(feature = "tokio-comp")]
    #[cfg_attr(docsrs, doc(cfg(feature = "tokio-comp")))]
    pub async fn get_multiplexed_tokio_connection_with_response_timeouts(
        &self,
        response_timeout: std::time::Duration,
        connection_timeout: std::time::Duration,
    ) -> RedisResult<crate::aio::MultiplexedConnection> {
        let result = Runtime::locate()
            .timeout(
                connection_timeout,
                self.get_multiplexed_async_connection_inner::<crate::aio::tokio::Tokio>(
                    &AsyncConnectionConfig::new().set_response_timeout(response_timeout),
                ),
            )
            .await;

        match result {
            Ok(Ok(connection)) => Ok(connection),
            Ok(Err(e)) => Err(e),
            Err(elapsed) => Err(elapsed.into()),
        }
    }

    /// Returns an async multiplexed connection from the client.
    ///
    /// A multiplexed connection can be cloned, allowing requests to be sent concurrently
    /// on the same underlying connection (tcp/unix socket).
    #[cfg(feature = "tokio-comp")]
    #[cfg_attr(docsrs, doc(cfg(feature = "tokio-comp")))]
    pub async fn get_multiplexed_tokio_connection(
        &self,
    ) -> RedisResult<crate::aio::MultiplexedConnection> {
        self.get_multiplexed_async_connection_inner::<crate::aio::tokio::Tokio>(
            &AsyncConnectionConfig::new(),
        )
        .await
    }

    /// Returns an async multiplexed connection from the client.
    ///
    /// A multiplexed connection can be cloned, allowing requests to be sent concurrently
    /// on the same underlying connection (tcp/unix socket).
    #[cfg(feature = "async-std-comp")]
    #[cfg_attr(docsrs, doc(cfg(feature = "async-std-comp")))]
    pub async fn get_multiplexed_async_std_connection_with_timeouts(
        &self,
        response_timeout: std::time::Duration,
        connection_timeout: std::time::Duration,
    ) -> RedisResult<crate::aio::MultiplexedConnection> {
        let result = Runtime::locate()
            .timeout(
                connection_timeout,
                self.get_multiplexed_async_connection_inner::<crate::aio::async_std::AsyncStd>(
                    &AsyncConnectionConfig::new().set_response_timeout(response_timeout),
                ),
            )
            .await;

        match result {
            Ok(Ok(connection)) => Ok(connection),
            Ok(Err(e)) => Err(e),
            Err(elapsed) => Err(elapsed.into()),
        }
    }

    /// Returns an async multiplexed connection from the client.
    ///
    /// A multiplexed connection can be cloned, allowing requests to be sent concurrently
    /// on the same underlying connection (tcp/unix socket).
    #[cfg(feature = "async-std-comp")]
    #[cfg_attr(docsrs, doc(cfg(feature = "async-std-comp")))]
    pub async fn get_multiplexed_async_std_connection(
        &self,
    ) -> RedisResult<crate::aio::MultiplexedConnection> {
        self.get_multiplexed_async_connection_inner::<crate::aio::async_std::AsyncStd>(
            &AsyncConnectionConfig::new(),
        )
        .await
    }

    /// Returns an async multiplexed connection from the client and a future which must be polled
    /// to drive any requests submitted to it (see [Self::get_multiplexed_async_connection]).
    ///
    /// A multiplexed connection can be cloned, allowing requests to be sent concurrently
    /// on the same underlying connection (tcp/unix socket).
    /// The multiplexer will return a timeout error on any request that takes longer then `response_timeout`.
    #[cfg(feature = "tokio-comp")]
    #[cfg_attr(docsrs, doc(cfg(feature = "tokio-comp")))]
    pub async fn create_multiplexed_tokio_connection_with_response_timeout(
        &self,
        response_timeout: std::time::Duration,
    ) -> RedisResult<(
        crate::aio::MultiplexedConnection,
        impl std::future::Future<Output = ()>,
    )> {
        self.create_multiplexed_async_connection_inner::<crate::aio::tokio::Tokio>(
            &AsyncConnectionConfig::new().set_response_timeout(response_timeout),
        )
        .await
    }

    /// Returns an async multiplexed connection from the client and a future which must be polled
    /// to drive any requests submitted to it (see [Self::get_multiplexed_async_connection]).
    ///
    /// A multiplexed connection can be cloned, allowing requests to be sent concurrently
    /// on the same underlying connection (tcp/unix socket).
    #[cfg(feature = "tokio-comp")]
    #[cfg_attr(docsrs, doc(cfg(feature = "tokio-comp")))]
    pub async fn create_multiplexed_tokio_connection(
        &self,
    ) -> RedisResult<(
        crate::aio::MultiplexedConnection,
        impl std::future::Future<Output = ()>,
    )> {
        self.create_multiplexed_async_connection_inner::<crate::aio::tokio::Tokio>(
            &AsyncConnectionConfig::new(),
        )
        .await
    }

    /// Returns an async multiplexed connection from the client and a future which must be polled
    /// to drive any requests submitted to it (see [Self::get_multiplexed_async_connection]).
    ///
    /// A multiplexed connection can be cloned, allowing requests to be sent concurrently
    /// on the same underlying connection (tcp/unix socket).
    /// The multiplexer will return a timeout error on any request that takes longer then `response_timeout`.
    #[cfg(feature = "async-std-comp")]
    #[cfg_attr(docsrs, doc(cfg(feature = "async-std-comp")))]
    pub async fn create_multiplexed_async_std_connection_with_response_timeout(
        &self,
        response_timeout: std::time::Duration,
    ) -> RedisResult<(
        crate::aio::MultiplexedConnection,
        impl std::future::Future<Output = ()>,
    )> {
        self.create_multiplexed_async_connection_inner::<crate::aio::async_std::AsyncStd>(
            &AsyncConnectionConfig::new().set_response_timeout(response_timeout),
        )
        .await
    }

    /// Returns an async multiplexed connection from the client and a future which must be polled
    /// to drive any requests submitted to it (see [Self::get_multiplexed_async_connection]).
    ///
    /// A multiplexed connection can be cloned, allowing requests to be sent concurrently
    /// on the same underlying connection (tcp/unix socket).
    #[cfg(feature = "async-std-comp")]
    #[cfg_attr(docsrs, doc(cfg(feature = "async-std-comp")))]
    pub async fn create_multiplexed_async_std_connection(
        &self,
    ) -> RedisResult<(
        crate::aio::MultiplexedConnection,
        impl std::future::Future<Output = ()>,
    )> {
        self.create_multiplexed_async_connection_inner::<crate::aio::async_std::AsyncStd>(
            &AsyncConnectionConfig::new(),
        )
        .await
    }

    /// Returns an async [`ConnectionManager`][connection-manager] from the client.
    ///
    /// The connection manager wraps a
    /// [`MultiplexedConnection`][multiplexed-connection]. If a command to that
    /// connection fails with a connection error, then a new connection is
    /// established in the background and the error is returned to the caller.
    ///
    /// This means that on connection loss at least one command will fail, but
    /// the connection will be re-established automatically if possible. Please
    /// refer to the [`ConnectionManager`][connection-manager] docs for
    /// detailed reconnecting behavior.
    ///
    /// A connection manager can be cloned, allowing requests to be sent concurrently
    /// on the same underlying connection (tcp/unix socket).
    ///
    /// [connection-manager]: aio/struct.ConnectionManager.html
    /// [multiplexed-connection]: aio/struct.MultiplexedConnection.html
    #[cfg(feature = "connection-manager")]
    #[cfg_attr(docsrs, doc(cfg(feature = "connection-manager")))]
    #[deprecated(note = "use get_connection_manager instead")]
    pub async fn get_tokio_connection_manager(&self) -> RedisResult<crate::aio::ConnectionManager> {
        crate::aio::ConnectionManager::new(self.clone()).await
    }

    /// Returns an async [`ConnectionManager`][connection-manager] from the client.
    ///
    /// The connection manager wraps a
    /// [`MultiplexedConnection`][multiplexed-connection]. If a command to that
    /// connection fails with a connection error, then a new connection is
    /// established in the background and the error is returned to the caller.
    ///
    /// This means that on connection loss at least one command will fail, but
    /// the connection will be re-established automatically if possible. Please
    /// refer to the [`ConnectionManager`][connection-manager] docs for
    /// detailed reconnecting behavior.
    ///
    /// A connection manager can be cloned, allowing requests to be sent concurrently
    /// on the same underlying connection (tcp/unix socket).
    ///
    /// [connection-manager]: aio/struct.ConnectionManager.html
    /// [multiplexed-connection]: aio/struct.MultiplexedConnection.html
    #[cfg(feature = "connection-manager")]
    #[cfg_attr(docsrs, doc(cfg(feature = "connection-manager")))]
    pub async fn get_connection_manager(&self) -> RedisResult<crate::aio::ConnectionManager> {
        crate::aio::ConnectionManager::new(self.clone()).await
    }

    /// Returns an async [`ConnectionManager`][connection-manager] from the client.
    ///
    /// The connection manager wraps a
    /// [`MultiplexedConnection`][multiplexed-connection]. If a command to that
    /// connection fails with a connection error, then a new connection is
    /// established in the background and the error is returned to the caller.
    ///
    /// This means that on connection loss at least one command will fail, but
    /// the connection will be re-established automatically if possible. Please
    /// refer to the [`ConnectionManager`][connection-manager] docs for
    /// detailed reconnecting behavior.
    ///
    /// A connection manager can be cloned, allowing requests to be sent concurrently
    /// on the same underlying connection (tcp/unix socket).
    ///
    /// [connection-manager]: aio/struct.ConnectionManager.html
    /// [multiplexed-connection]: aio/struct.MultiplexedConnection.html
    #[cfg(feature = "connection-manager")]
    #[cfg_attr(docsrs, doc(cfg(feature = "connection-manager")))]
    #[deprecated(note = "Use `get_connection_manager_with_config` instead")]
    pub async fn get_tokio_connection_manager_with_backoff(
        &self,
        exponent_base: u64,
        factor: u64,
        number_of_retries: usize,
    ) -> RedisResult<crate::aio::ConnectionManager> {
        use crate::aio::ConnectionManagerConfig;

        let config = ConnectionManagerConfig::new()
            .set_exponent_base(exponent_base)
            .set_factor(factor)
            .set_number_of_retries(number_of_retries);
        crate::aio::ConnectionManager::new_with_config(self.clone(), config).await
    }

    /// Returns an async [`ConnectionManager`][connection-manager] from the client.
    ///
    /// The connection manager wraps a
    /// [`MultiplexedConnection`][multiplexed-connection]. If a command to that
    /// connection fails with a connection error, then a new connection is
    /// established in the background and the error is returned to the caller.
    ///
    /// This means that on connection loss at least one command will fail, but
    /// the connection will be re-established automatically if possible. Please
    /// refer to the [`ConnectionManager`][connection-manager] docs for
    /// detailed reconnecting behavior.
    ///
    /// A connection manager can be cloned, allowing requests to be sent concurrently
    /// on the same underlying connection (tcp/unix socket).
    ///
    /// [connection-manager]: aio/struct.ConnectionManager.html
    /// [multiplexed-connection]: aio/struct.MultiplexedConnection.html
    #[cfg(feature = "connection-manager")]
    #[cfg_attr(docsrs, doc(cfg(feature = "connection-manager")))]
    #[deprecated(note = "Use `get_connection_manager_with_config` instead")]
    pub async fn get_tokio_connection_manager_with_backoff_and_timeouts(
        &self,
        exponent_base: u64,
        factor: u64,
        number_of_retries: usize,
        response_timeout: std::time::Duration,
        connection_timeout: std::time::Duration,
    ) -> RedisResult<crate::aio::ConnectionManager> {
        use crate::aio::ConnectionManagerConfig;

        let config = ConnectionManagerConfig::new()
            .set_exponent_base(exponent_base)
            .set_factor(factor)
            .set_response_timeout(response_timeout)
            .set_connection_timeout(connection_timeout)
            .set_number_of_retries(number_of_retries);
        crate::aio::ConnectionManager::new_with_config(self.clone(), config).await
    }

    /// Returns an async [`ConnectionManager`][connection-manager] from the client.
    ///
    /// The connection manager wraps a
    /// [`MultiplexedConnection`][multiplexed-connection]. If a command to that
    /// connection fails with a connection error, then a new connection is
    /// established in the background and the error is returned to the caller.
    ///
    /// This means that on connection loss at least one command will fail, but
    /// the connection will be re-established automatically if possible. Please
    /// refer to the [`ConnectionManager`][connection-manager] docs for
    /// detailed reconnecting behavior.
    ///
    /// A connection manager can be cloned, allowing requests to be sent concurrently
    /// on the same underlying connection (tcp/unix socket).
    ///
    /// [connection-manager]: aio/struct.ConnectionManager.html
    /// [multiplexed-connection]: aio/struct.MultiplexedConnection.html
    #[cfg(feature = "connection-manager")]
    #[cfg_attr(docsrs, doc(cfg(feature = "connection-manager")))]
    #[deprecated(note = "Use `get_connection_manager_with_config` instead")]
    pub async fn get_connection_manager_with_backoff_and_timeouts(
        &self,
        exponent_base: u64,
        factor: u64,
        number_of_retries: usize,
        response_timeout: std::time::Duration,
        connection_timeout: std::time::Duration,
    ) -> RedisResult<crate::aio::ConnectionManager> {
        use crate::aio::ConnectionManagerConfig;

        let config = ConnectionManagerConfig::new()
            .set_exponent_base(exponent_base)
            .set_factor(factor)
            .set_response_timeout(response_timeout)
            .set_connection_timeout(connection_timeout)
            .set_number_of_retries(number_of_retries);
        crate::aio::ConnectionManager::new_with_config(self.clone(), config).await
    }

    /// Returns an async [`ConnectionManager`][connection-manager] from the client.
    ///
    /// The connection manager wraps a
    /// [`MultiplexedConnection`][multiplexed-connection]. If a command to that
    /// connection fails with a connection error, then a new connection is
    /// established in the background and the error is returned to the caller.
    ///
    /// This means that on connection loss at least one command will fail, but
    /// the connection will be re-established automatically if possible. Please
    /// refer to the [`ConnectionManager`][connection-manager] docs for
    /// detailed reconnecting behavior.
    ///
    /// A connection manager can be cloned, allowing requests to be sent concurrently
    /// on the same underlying connection (tcp/unix socket).
    ///
    /// [connection-manager]: aio/struct.ConnectionManager.html
    /// [multiplexed-connection]: aio/struct.MultiplexedConnection.html
    #[cfg(feature = "connection-manager")]
    #[cfg_attr(docsrs, doc(cfg(feature = "connection-manager")))]
    pub async fn get_connection_manager_with_config(
        &self,
        config: crate::aio::ConnectionManagerConfig,
    ) -> RedisResult<crate::aio::ConnectionManager> {
        crate::aio::ConnectionManager::new_with_config(self.clone(), config).await
    }

    /// Returns an async [`ConnectionManager`][connection-manager] from the client.
    ///
    /// The connection manager wraps a
    /// [`MultiplexedConnection`][multiplexed-connection]. If a command to that
    /// connection fails with a connection error, then a new connection is
    /// established in the background and the error is returned to the caller.
    ///
    /// This means that on connection loss at least one command will fail, but
    /// the connection will be re-established automatically if possible. Please
    /// refer to the [`ConnectionManager`][connection-manager] docs for
    /// detailed reconnecting behavior.
    ///
    /// A connection manager can be cloned, allowing requests to be be sent concurrently
    /// on the same underlying connection (tcp/unix socket).
    ///
    /// [connection-manager]: aio/struct.ConnectionManager.html
    /// [multiplexed-connection]: aio/struct.MultiplexedConnection.html
    #[cfg(feature = "connection-manager")]
    #[cfg_attr(docsrs, doc(cfg(feature = "connection-manager")))]
    #[deprecated(note = "Use `get_connection_manager_with_config` instead")]
    pub async fn get_connection_manager_with_backoff(
        &self,
        exponent_base: u64,
        factor: u64,
        number_of_retries: usize,
    ) -> RedisResult<crate::aio::ConnectionManager> {
        use crate::aio::ConnectionManagerConfig;

        let config = ConnectionManagerConfig::new()
            .set_exponent_base(exponent_base)
            .set_factor(factor)
            .set_number_of_retries(number_of_retries);
        crate::aio::ConnectionManager::new_with_config(self.clone(), config).await
    }

    async fn get_multiplexed_async_connection_inner_with_timeout<T>(
        &self,
        config: &AsyncConnectionConfig,
        rt: Runtime,
    ) -> RedisResult<crate::aio::MultiplexedConnection>
    where
        T: crate::aio::RedisRuntime,
    {
        let result = if let Some(connection_timeout) = config.connection_timeout {
            rt.timeout(
                connection_timeout,
                self.get_multiplexed_async_connection_inner::<T>(config),
            )
            .await
        } else {
            Ok(self
                .get_multiplexed_async_connection_inner::<T>(config)
                .await)
        };

        match result {
            Ok(Ok(connection)) => Ok(connection),
            Ok(Err(e)) => Err(e),
            Err(elapsed) => Err(elapsed.into()),
        }
    }

    async fn get_multiplexed_async_connection_inner<T>(
        &self,
        config: &AsyncConnectionConfig,
    ) -> RedisResult<crate::aio::MultiplexedConnection>
    where
        T: crate::aio::RedisRuntime,
    {
        let (mut connection, driver) = self
            .create_multiplexed_async_connection_inner::<T>(config)
            .await?;
        let handle = T::spawn(driver);
        connection.set_task_handle(handle);
        Ok(connection)
    }

    async fn create_multiplexed_async_connection_inner<T>(
        &self,
        config: &AsyncConnectionConfig,
    ) -> RedisResult<(
        crate::aio::MultiplexedConnection,
        impl std::future::Future<Output = ()>,
    )>
    where
        T: crate::aio::RedisRuntime,
    {
        let resolver = config
            .dns_resolver
            .as_deref()
            .unwrap_or(&DefaultAsyncDNSResolver);
        let con = self
            .get_simple_async_connection::<T>(resolver, &config.tcp_settings)
            .await?;
        crate::aio::MultiplexedConnection::new_with_config(
            &self.connection_info.redis,
            con,
            config.clone(),
        )
        .await
    }

    async fn get_simple_async_connection_dynamically(
        &self,
        dns_resolver: &dyn AsyncDNSResolver,
        tcp_settings: &TcpSettings,
    ) -> RedisResult<Pin<Box<dyn crate::aio::AsyncStream + Send + Sync>>> {
        match Runtime::locate() {
            #[cfg(feature = "tokio-comp")]
            Runtime::Tokio => {
                self.get_simple_async_connection::<crate::aio::tokio::Tokio>(
                    dns_resolver,
                    tcp_settings,
                )
                .await
            }

            #[cfg(feature = "async-std-comp")]
            Runtime::AsyncStd => {
                self.get_simple_async_connection::<crate::aio::async_std::AsyncStd>(
                    dns_resolver,
                    tcp_settings,
                )
                .await
            }

            #[cfg(feature = "smol-comp")]
            Runtime::Smol => {
                self.get_simple_async_connection::<crate::aio::smol::Smol>(
                    dns_resolver,
                    tcp_settings,
                )
                .await
            }
        }
    }

    async fn get_simple_async_connection<T>(
        &self,
        dns_resolver: &dyn AsyncDNSResolver,
        tcp_settings: &TcpSettings,
    ) -> RedisResult<Pin<Box<dyn crate::aio::AsyncStream + Send + Sync>>>
    where
        T: crate::aio::RedisRuntime,
    {
        Ok(
            crate::aio::connect_simple::<T>(&self.connection_info, dns_resolver, tcp_settings)
                .await?
                .boxed(),
        )
    }

    #[cfg(feature = "connection-manager")]
    pub(crate) fn connection_info(&self) -> &ConnectionInfo {
        &self.connection_info
    }

    /// Returns an async receiver for pub-sub messages.
    #[cfg(feature = "aio")]
    // TODO - do we want to type-erase pubsub using a trait, to allow us to replace it with a different implementation later?
    pub async fn get_async_pubsub(&self) -> RedisResult<crate::aio::PubSub> {
        let connection = self
            .get_simple_async_connection_dynamically(
                &DefaultAsyncDNSResolver,
                &TcpSettings::default(),
            )
            .await?;

        crate::aio::PubSub::new(&self.connection_info.redis, connection).await
    }

    /// Returns an async receiver for monitor messages.
    #[cfg(feature = "aio")]
    pub async fn get_async_monitor(&self) -> RedisResult<crate::aio::Monitor> {
        let connection = self
            .get_simple_async_connection_dynamically(
                &DefaultAsyncDNSResolver,
                &TcpSettings::default(),
            )
            .await?;
        crate::aio::Monitor::new(&self.connection_info.redis, connection).await
    }
}

#[cfg(feature = "aio")]
use crate::aio::Runtime;

impl ConnectionLike for Client {
    fn req_packed_command(&mut self, cmd: &[u8]) -> RedisResult<Value> {
        self.get_connection()?.req_packed_command(cmd)
    }

    fn req_packed_commands(
        &mut self,
        cmd: &[u8],
        offset: usize,
        count: usize,
    ) -> RedisResult<Vec<Value>> {
        self.get_connection()?
            .req_packed_commands(cmd, offset, count)
    }

    fn get_db(&self) -> i64 {
        self.connection_info.redis.db
    }

    fn check_connection(&mut self) -> bool {
        if let Ok(mut conn) = self.get_connection() {
            conn.check_connection()
        } else {
            false
        }
    }

    fn is_open(&self) -> bool {
        if let Ok(conn) = self.get_connection() {
            conn.is_open()
        } else {
            false
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn regression_293_parse_ipv6_with_interface() {
        assert!(Client::open(("fe80::cafe:beef%eno1", 6379)).is_ok());
    }
}
