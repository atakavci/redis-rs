#[cfg(feature = "cluster-async")]
use crate::aio::AsyncPushSender;
#[cfg(all(feature = "cache-aio", feature = "cluster-async"))]
use crate::caching::{CacheConfig, CacheManager};
use crate::connection::{ConnectionAddr, ConnectionInfo, IntoConnectionInfo};
#[cfg(feature = "cluster-async")]
use crate::io::{tcp::TcpSettings, AsyncDNSResolver};
use crate::types::{ErrorKind, ProtocolVersion, RedisError, RedisResult};
use crate::{cluster, cluster::TlsMode};
use rand::Rng;
#[cfg(feature = "cluster-async")]
use std::sync::Arc;
use std::time::Duration;

use crate::connection::TlsConnParams;

#[cfg(feature = "cluster-async")]
use crate::cluster_async;

#[cfg(feature = "tls-rustls")]
use crate::tls::{retrieve_tls_certificates, TlsCertificates};

/// Parameters specific to builder, so that
/// builder parameters may have different types
/// than final ClusterParams
#[derive(Default)]
struct BuilderParams {
    password: Option<String>,
    username: Option<String>,
    read_from_replicas: bool,
    tls: Option<TlsMode>,
    #[cfg(feature = "tls-rustls")]
    certs: Option<TlsCertificates>,
    #[cfg(any(feature = "tls-rustls-insecure", feature = "tls-native-tls"))]
    danger_accept_invalid_hostnames: bool,
    retries_configuration: RetryParams,
    connection_timeout: Option<Duration>,
    response_timeout: Option<Duration>,
    protocol: Option<ProtocolVersion>,
    #[cfg(feature = "cluster-async")]
    async_push_sender: Option<Arc<dyn AsyncPushSender>>,
    #[cfg(feature = "cluster-async")]
    pub(crate) tcp_settings: TcpSettings,
    #[cfg(feature = "cluster-async")]
    async_dns_resolver: Option<Arc<dyn AsyncDNSResolver>>,
    #[cfg(feature = "cache-aio")]
    cache_config: Option<CacheConfig>,
}

#[derive(Clone)]
pub(crate) struct RetryParams {
    pub(crate) number_of_retries: u32,
    max_wait_time: u64,
    min_wait_time: u64,
    exponent_base: u64,
    factor: u64,
}

impl Default for RetryParams {
    fn default() -> Self {
        const DEFAULT_RETRIES: u32 = 16;
        const DEFAULT_MAX_RETRY_WAIT_TIME: u64 = 655360;
        const DEFAULT_MIN_RETRY_WAIT_TIME: u64 = 1280;
        const DEFAULT_EXPONENT_BASE: u64 = 2;
        const DEFAULT_FACTOR: u64 = 10;
        Self {
            number_of_retries: DEFAULT_RETRIES,
            max_wait_time: DEFAULT_MAX_RETRY_WAIT_TIME,
            min_wait_time: DEFAULT_MIN_RETRY_WAIT_TIME,
            exponent_base: DEFAULT_EXPONENT_BASE,
            factor: DEFAULT_FACTOR,
        }
    }
}

impl RetryParams {
    pub(crate) fn wait_time_for_retry(&self, retry: u32) -> Duration {
        let base_wait = self.exponent_base.pow(retry) * self.factor;
        let clamped_wait = base_wait
            .min(self.max_wait_time)
            .max(self.min_wait_time + 1);
        let jittered_wait = rand::rng().random_range(self.min_wait_time..clamped_wait);
        Duration::from_millis(jittered_wait)
    }
}

/// Redis cluster specific parameters.
#[derive(Default, Clone)]
pub(crate) struct ClusterParams {
    pub(crate) password: Option<String>,
    pub(crate) username: Option<String>,
    pub(crate) read_from_replicas: bool,
    /// tls indicates tls behavior of connections.
    /// When Some(TlsMode), connections use tls and verify certification depends on TlsMode.
    /// When None, connections do not use tls.
    pub(crate) tls: Option<TlsMode>,
    pub(crate) retry_params: RetryParams,
    pub(crate) tls_params: Option<TlsConnParams>,
    pub(crate) connection_timeout: Duration,
    pub(crate) response_timeout: Option<Duration>,
    pub(crate) protocol: Option<ProtocolVersion>,
    #[cfg(feature = "cluster-async")]
    pub(crate) async_push_sender: Option<Arc<dyn AsyncPushSender>>,
    #[cfg(feature = "cluster-async")]
    pub(crate) tcp_settings: TcpSettings,
    #[cfg(feature = "cluster-async")]
    pub(crate) async_dns_resolver: Option<Arc<dyn AsyncDNSResolver>>,
    #[cfg(all(feature = "cache-aio", feature = "cluster-async"))]
    pub(crate) cache_manager: Option<CacheManager>,
}

impl ClusterParams {
    fn from(value: BuilderParams) -> RedisResult<Self> {
        #[cfg(not(feature = "tls-rustls"))]
        let tls_params: Option<TlsConnParams> = None;

        #[cfg(feature = "tls-rustls")]
        let tls_params = {
            let retrieved_tls_params = value.certs.as_ref().map(retrieve_tls_certificates);

            retrieved_tls_params.transpose()?
        };

        #[cfg(any(feature = "tls-rustls-insecure", feature = "tls-native-tls"))]
        let tls_params = if value.danger_accept_invalid_hostnames {
            let mut tls_params = tls_params.unwrap_or(TlsConnParams {
                #[cfg(feature = "tls-rustls")]
                client_tls_params: None,
                #[cfg(feature = "tls-rustls")]
                root_cert_store: None,
                danger_accept_invalid_hostnames: false,
            });
            tls_params.danger_accept_invalid_hostnames = true;
            Some(tls_params)
        } else {
            tls_params
        };

        #[cfg(all(feature = "cache-aio", feature = "cluster-async"))]
        let cache_manager = value
            .cache_config
            .as_ref()
            .map(|cache_config| CacheManager::new(*cache_config));

        Ok(Self {
            password: value.password,
            username: value.username,
            read_from_replicas: value.read_from_replicas,
            tls: value.tls,
            retry_params: value.retries_configuration,
            tls_params,
            connection_timeout: value.connection_timeout.unwrap_or(Duration::from_secs(1)),
            response_timeout: value.response_timeout,
            protocol: value.protocol,
            #[cfg(feature = "cluster-async")]
            async_push_sender: value.async_push_sender,
            #[cfg(feature = "cluster-async")]
            tcp_settings: value.tcp_settings,
            #[cfg(feature = "cluster-async")]
            async_dns_resolver: value.async_dns_resolver,
            #[cfg(all(feature = "cache-aio", feature = "cluster-async"))]
            cache_manager,
        })
    }

    fn with_config(mut self, config: cluster::ClusterConfig) -> Self {
        if let Some(connection_timeout) = config.connection_timeout {
            self.connection_timeout = connection_timeout;
        }
        self.response_timeout = config.response_timeout;

        #[cfg(feature = "cluster-async")]
        if let Some(async_push_sender) = config.async_push_sender {
            self.async_push_sender = Some(async_push_sender);
        }

        #[cfg(feature = "cluster-async")]
        if let Some(async_dns_resolver) = config.async_dns_resolver {
            self.async_dns_resolver = Some(async_dns_resolver);
        }

        self
    }
}

/// Used to configure and build a [`ClusterClient`].
pub struct ClusterClientBuilder {
    initial_nodes: RedisResult<Vec<ConnectionInfo>>,
    builder_params: BuilderParams,
}

impl ClusterClientBuilder {
    /// Creates a new `ClusterClientBuilder` with the provided initial_nodes.
    ///
    /// This is the same as `ClusterClient::builder(initial_nodes)`.
    pub fn new<T: IntoConnectionInfo>(
        initial_nodes: impl IntoIterator<Item = T>,
    ) -> ClusterClientBuilder {
        ClusterClientBuilder {
            initial_nodes: initial_nodes
                .into_iter()
                .map(|x| x.into_connection_info())
                .collect(),
            builder_params: Default::default(),
        }
    }

    /// Creates a new [`ClusterClient`] from the parameters.
    ///
    /// This does not create connections to the Redis Cluster, but only performs some basic checks
    /// on the initial nodes' URLs and passwords/usernames.
    ///
    /// When the `tls-rustls` feature is enabled and TLS credentials are provided, they are set for
    /// each cluster connection.
    ///
    /// # Errors
    ///
    /// Upon failure to parse initial nodes or if the initial nodes have different passwords or
    /// usernames, an error is returned.
    pub fn build(self) -> RedisResult<ClusterClient> {
        let initial_nodes = self.initial_nodes?;

        let first_node = match initial_nodes.first() {
            Some(node) => node,
            None => {
                return Err(RedisError::from((
                    ErrorKind::InvalidClientConfig,
                    "Initial nodes can't be empty.",
                )))
            }
        };

        let mut cluster_params = ClusterParams::from(self.builder_params)?;
        let password = if cluster_params.password.is_none() {
            cluster_params
                .password
                .clone_from(&first_node.redis.password);
            &cluster_params.password
        } else {
            &None
        };
        let username = if cluster_params.username.is_none() {
            cluster_params
                .username
                .clone_from(&first_node.redis.username);
            &cluster_params.username
        } else {
            &None
        };
        let tls = if cluster_params.tls.is_none() {
            cluster_params.tls = first_node.addr.tls_mode();
            cluster_params.tls
        } else {
            None
        };
        let protocol = if cluster_params.protocol.is_none() {
            cluster_params.protocol = Some(first_node.redis.protocol);
            cluster_params.protocol
        } else {
            None
        };

        // Verify that the initial nodes match the cluster client's configuration.
        for node in &initial_nodes {
            if let ConnectionAddr::Unix(_) = node.addr {
                return Err(RedisError::from((ErrorKind::InvalidClientConfig,
                                             "This library cannot use unix socket because Redis's cluster command returns only cluster's IP and port.")));
            }

            if password.is_some() && node.redis.password != *password {
                return Err(RedisError::from((
                    ErrorKind::InvalidClientConfig,
                    "Cannot use different password among initial nodes.",
                )));
            }

            if username.is_some() && node.redis.username != *username {
                return Err(RedisError::from((
                    ErrorKind::InvalidClientConfig,
                    "Cannot use different username among initial nodes.",
                )));
            }
            if protocol.is_some() && Some(node.redis.protocol) != protocol {
                return Err(RedisError::from((
                    ErrorKind::InvalidClientConfig,
                    "Cannot use different protocol among initial nodes.",
                )));
            }

            if tls.is_some() && node.addr.tls_mode() != tls {
                return Err(RedisError::from((
                    ErrorKind::InvalidClientConfig,
                    "Cannot use different TLS modes among initial nodes.",
                )));
            }
        }

        Ok(ClusterClient {
            initial_nodes,
            cluster_params,
        })
    }

    /// Sets password for the new ClusterClient.
    pub fn password(mut self, password: String) -> ClusterClientBuilder {
        self.builder_params.password = Some(password);
        self
    }

    /// Sets username for the new ClusterClient.
    pub fn username(mut self, username: String) -> ClusterClientBuilder {
        self.builder_params.username = Some(username);
        self
    }

    /// Sets number of retries for the new ClusterClient.
    pub fn retries(mut self, retries: u32) -> ClusterClientBuilder {
        self.builder_params.retries_configuration.number_of_retries = retries;
        self
    }

    /// Sets maximal wait time in millisceonds between retries for the new ClusterClient.
    pub fn max_retry_wait(mut self, max_wait: u64) -> ClusterClientBuilder {
        self.builder_params.retries_configuration.max_wait_time = max_wait;
        self
    }

    /// Sets minimal wait time in millisceonds between retries for the new ClusterClient.
    pub fn min_retry_wait(mut self, min_wait: u64) -> ClusterClientBuilder {
        self.builder_params.retries_configuration.min_wait_time = min_wait;
        self
    }

    /// Sets the factor and exponent base for the retry wait time.
    /// The formula for the wait is rand(min_wait_retry .. min(max_retry_wait , factor * exponent_base ^ retry))ms.
    pub fn retry_wait_formula(mut self, factor: u64, exponent_base: u64) -> ClusterClientBuilder {
        self.builder_params.retries_configuration.factor = factor;
        self.builder_params.retries_configuration.exponent_base = exponent_base;
        self
    }

    /// Sets TLS mode for the new ClusterClient.
    ///
    /// It is extracted from the first node of initial_nodes if not set.
    #[cfg(any(feature = "tls-native-tls", feature = "tls-rustls"))]
    pub fn tls(mut self, tls: TlsMode) -> ClusterClientBuilder {
        self.builder_params.tls = Some(tls);
        self
    }

    /// Configure hostname verification when connecting with TLS.
    ///
    /// If `insecure` is true, this **disables** hostname verification, while
    /// leaving other aspects of certificate checking enabled. This mode is
    /// similar to what `redis-cli` does: TLS connections do check certificates,
    /// but hostname errors are ignored.
    ///
    /// # Warning
    ///
    /// You should think very carefully before you use this method. If hostname
    /// verification is not used, any valid certificate for any site will be
    /// trusted for use from any other. This introduces a significant
    /// vulnerability to man-in-the-middle attacks.
    #[cfg(any(feature = "tls-rustls-insecure", feature = "tls-native-tls"))]
    pub fn danger_accept_invalid_hostnames(mut self, insecure: bool) -> ClusterClientBuilder {
        self.builder_params.danger_accept_invalid_hostnames = insecure;
        self
    }

    /// Sets raw TLS certificates for the new ClusterClient.
    ///
    /// When set, enforces the connection must be TLS secured.
    ///
    /// All certificates must be provided as byte streams loaded from PEM files their consistency is
    /// checked during `build()` call.
    ///
    /// - `certificates` - `TlsCertificates` structure containing:
    ///     - `client_tls` - Optional `ClientTlsConfig` containing byte streams for
    ///         - `client_cert` - client's byte stream containing client certificate in PEM format
    ///         - `client_key` - client's byte stream containing private key in PEM format
    ///
    ///     - `root_cert` - Optional byte stream yielding PEM formatted file for root certificates.
    ///
    /// If `ClientTlsConfig` ( cert+key pair ) is not provided, then client-side authentication is not enabled.
    /// If `root_cert` is not provided, then system root certificates are used instead.
    #[cfg(feature = "tls-rustls")]
    pub fn certs(mut self, certificates: TlsCertificates) -> ClusterClientBuilder {
        if self.builder_params.tls.is_none() {
            self.builder_params.tls = Some(TlsMode::Secure);
        }

        self.builder_params.certs = Some(certificates);
        self
    }

    /// Enables reading from replicas for all new connections (default is disabled).
    ///
    /// If enabled, then read queries will go to the replica nodes & write queries will go to the
    /// primary nodes. If there are no replica nodes, then all queries will go to the primary nodes.
    pub fn read_from_replicas(mut self) -> ClusterClientBuilder {
        self.builder_params.read_from_replicas = true;
        self
    }

    /// Enables timing out on slow connection time.
    ///
    /// If enabled, the cluster will only wait the given time on each connection attempt to each node.
    pub fn connection_timeout(mut self, connection_timeout: Duration) -> ClusterClientBuilder {
        self.builder_params.connection_timeout = Some(connection_timeout);
        self
    }

    /// Enables timing out on slow responses.
    ///
    /// If enabled, the cluster will only wait the given time to each response from each node.
    pub fn response_timeout(mut self, response_timeout: Duration) -> ClusterClientBuilder {
        self.builder_params.response_timeout = Some(response_timeout);
        self
    }

    /// Sets the protocol with which the client should communicate with the server.
    pub fn use_protocol(mut self, protocol: ProtocolVersion) -> ClusterClientBuilder {
        self.builder_params.protocol = Some(protocol);
        self
    }

    /// Use `build()`.
    #[deprecated(since = "0.22.0", note = "Use build()")]
    pub fn open(self) -> RedisResult<ClusterClient> {
        self.build()
    }

    /// Use `read_from_replicas()`.
    #[deprecated(since = "0.22.0", note = "Use read_from_replicas()")]
    pub fn readonly(mut self, read_from_replicas: bool) -> ClusterClientBuilder {
        self.builder_params.read_from_replicas = read_from_replicas;
        self
    }

    #[cfg(feature = "cluster-async")]
    /// Sets sender sender for push values.
    ///
    /// The sender can be a channel, or an arbitrary function that handles [crate::PushInfo] values.
    /// This will fail client creation if the connection isn't configured for RESP3 communications via the [crate::RedisConnectionInfo::protocol] field.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use redis::cluster::ClusterClientBuilder;
    /// let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    /// let config = ClusterClientBuilder::new(vec!["redis://127.0.0.1:6379/"])
    ///     .use_protocol(redis::ProtocolVersion::RESP3)
    ///     .push_sender(tx);
    /// ```
    ///
    /// ```rust
    /// # use std::sync::{Mutex, Arc};
    /// # use redis::cluster::ClusterClientBuilder;
    /// let messages = Arc::new(Mutex::new(Vec::new()));
    /// let config = ClusterClientBuilder::new(vec!["redis://127.0.0.1:6379/"])
    ///     .use_protocol(redis::ProtocolVersion::RESP3)
    ///     .push_sender(move |msg|{
    ///         let Ok(mut messages) = messages.lock() else {
    ///             return Err(redis::aio::SendError);
    ///         };
    ///         messages.push(msg);
    ///         Ok(())
    ///     });
    /// ```
    pub fn push_sender(mut self, push_sender: impl AsyncPushSender) -> ClusterClientBuilder {
        self.builder_params.async_push_sender = Some(Arc::new(push_sender));
        self
    }

    /// Set the behavior of the underlying TCP connection.
    #[cfg(feature = "cluster-async")]
    pub fn tcp_settings(mut self, tcp_settings: TcpSettings) -> ClusterClientBuilder {
        self.builder_params.tcp_settings = tcp_settings;
        self
    }

    /// Set asynchronous DNS resolver for the underlying TCP connection.
    ///
    /// The parameter resolver must implement the [`crate::io::AsyncDNSResolver`] trait.
    #[cfg(feature = "cluster-async")]
    pub fn async_dns_resolver(mut self, resolver: impl AsyncDNSResolver) -> ClusterClientBuilder {
        self.builder_params.async_dns_resolver = Some(Arc::new(resolver));
        self
    }

    /// Sets cache config for [`crate::cluster_async::ClusterConnection`], check CacheConfig for more details.
    #[cfg(all(feature = "cache-aio", feature = "cluster-async"))]
    pub fn cache_config(mut self, cache_config: CacheConfig) -> Self {
        self.builder_params.cache_config = Some(cache_config);
        self
    }
}

/// A Redis Cluster client, used to create connections.
#[derive(Clone)]
pub struct ClusterClient {
    initial_nodes: Vec<ConnectionInfo>,
    cluster_params: ClusterParams,
}

impl ClusterClient {
    /// Creates a `ClusterClient` with the default parameters.
    ///
    /// This does not create connections to the Redis Cluster, but only performs some basic checks
    /// on the initial nodes' URLs and passwords/usernames.
    ///
    /// # Errors
    ///
    /// Upon failure to parse initial nodes or if the initial nodes have different passwords or
    /// usernames, an error is returned.
    pub fn new<T: IntoConnectionInfo>(
        initial_nodes: impl IntoIterator<Item = T>,
    ) -> RedisResult<ClusterClient> {
        Self::builder(initial_nodes).build()
    }

    /// Creates a [`ClusterClientBuilder`] with the provided initial_nodes.
    pub fn builder<T: IntoConnectionInfo>(
        initial_nodes: impl IntoIterator<Item = T>,
    ) -> ClusterClientBuilder {
        ClusterClientBuilder::new(initial_nodes)
    }

    /// Creates new connections to Redis Cluster nodes and returns a
    /// [`cluster::ClusterConnection`].
    ///
    /// # Errors
    ///
    /// An error is returned if there is a failure while creating connections or slots.
    pub fn get_connection(&self) -> RedisResult<cluster::ClusterConnection> {
        cluster::ClusterConnection::new(self.cluster_params.clone(), self.initial_nodes.clone())
    }

    /// Creates new connections to Redis Cluster nodes with a custom config and returns a
    /// [`cluster_async::ClusterConnection`].
    ///
    /// # Errors
    ///
    /// An error is returned if there is a failure while creating connections or slots.
    pub fn get_connection_with_config(
        &self,
        config: cluster::ClusterConfig,
    ) -> RedisResult<cluster::ClusterConnection> {
        cluster::ClusterConnection::new(
            self.cluster_params.clone().with_config(config),
            self.initial_nodes.clone(),
        )
    }

    /// Creates new connections to Redis Cluster nodes and returns a
    /// [`cluster_async::ClusterConnection`].
    ///
    /// # Errors
    ///
    /// An error is returned if there is a failure while creating connections or slots.
    #[cfg(feature = "cluster-async")]
    pub async fn get_async_connection(&self) -> RedisResult<cluster_async::ClusterConnection> {
        cluster_async::ClusterConnection::new(&self.initial_nodes, self.cluster_params.clone())
            .await
    }

    /// Creates new connections to Redis Cluster nodes with a custom config and returns a
    /// [`cluster_async::ClusterConnection`].
    ///
    /// # Errors
    ///
    /// An error is returned if there is a failure while creating connections or slots.
    #[cfg(feature = "cluster-async")]
    pub async fn get_async_connection_with_config(
        &self,
        config: cluster::ClusterConfig,
    ) -> RedisResult<cluster_async::ClusterConnection> {
        cluster_async::ClusterConnection::new(
            &self.initial_nodes,
            self.cluster_params.clone().with_config(config),
        )
        .await
    }

    #[doc(hidden)]
    pub fn get_generic_connection<C>(&self) -> RedisResult<cluster::ClusterConnection<C>>
    where
        C: crate::ConnectionLike + crate::cluster::Connect + Send,
    {
        cluster::ClusterConnection::new(self.cluster_params.clone(), self.initial_nodes.clone())
    }

    #[doc(hidden)]
    #[cfg(feature = "cluster-async")]
    pub async fn get_async_generic_connection<C>(
        &self,
    ) -> RedisResult<cluster_async::ClusterConnection<C>>
    where
        C: crate::aio::ConnectionLike
            + cluster_async::Connect
            + Clone
            + Send
            + Sync
            + Unpin
            + 'static,
    {
        cluster_async::ClusterConnection::new(&self.initial_nodes, self.cluster_params.clone())
            .await
    }

    /// Use `new()`.
    #[deprecated(since = "0.22.0", note = "Use new()")]
    pub fn open<T: IntoConnectionInfo>(initial_nodes: Vec<T>) -> RedisResult<ClusterClient> {
        Self::new(initial_nodes)
    }
}

#[cfg(test)]
mod tests {
    use super::{ClusterClient, ClusterClientBuilder, ConnectionInfo, IntoConnectionInfo};

    fn get_connection_data() -> Vec<ConnectionInfo> {
        vec![
            "redis://127.0.0.1:6379".into_connection_info().unwrap(),
            "redis://127.0.0.1:6378".into_connection_info().unwrap(),
            "redis://127.0.0.1:6377".into_connection_info().unwrap(),
        ]
    }

    fn get_connection_data_with_password() -> Vec<ConnectionInfo> {
        vec![
            "redis://:password@127.0.0.1:6379"
                .into_connection_info()
                .unwrap(),
            "redis://:password@127.0.0.1:6378"
                .into_connection_info()
                .unwrap(),
            "redis://:password@127.0.0.1:6377"
                .into_connection_info()
                .unwrap(),
        ]
    }

    fn get_connection_data_with_username_and_password() -> Vec<ConnectionInfo> {
        vec![
            "redis://user1:password@127.0.0.1:6379"
                .into_connection_info()
                .unwrap(),
            "redis://user1:password@127.0.0.1:6378"
                .into_connection_info()
                .unwrap(),
            "redis://user1:password@127.0.0.1:6377"
                .into_connection_info()
                .unwrap(),
        ]
    }

    #[test]
    fn give_no_password() {
        let client = ClusterClient::new(get_connection_data()).unwrap();
        assert_eq!(client.cluster_params.password, None);
    }

    #[test]
    fn give_password_by_initial_nodes() {
        let client = ClusterClient::new(get_connection_data_with_password()).unwrap();
        assert_eq!(client.cluster_params.password, Some("password".to_string()));
    }

    #[test]
    fn give_username_and_password_by_initial_nodes() {
        let client = ClusterClient::new(get_connection_data_with_username_and_password()).unwrap();
        assert_eq!(client.cluster_params.password, Some("password".to_string()));
        assert_eq!(client.cluster_params.username, Some("user1".to_string()));
    }

    #[test]
    fn fail_if_received_different_password_between_initial_nodes() {
        let result = ClusterClient::new(vec![
            "redis://:password1@127.0.0.1:6379",
            "redis://:password2@127.0.0.1:6378",
            "redis://:password3@127.0.0.1:6377",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn fail_if_received_different_username_between_initial_nodes() {
        let result = ClusterClient::new(vec![
            "redis://user1:password@127.0.0.1:6379",
            "redis://user2:password@127.0.0.1:6378",
            "redis://user1:password@127.0.0.1:6377",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn fail_if_received_different_protocol_between_initial_nodes() {
        let result = ClusterClient::new(vec![
            "redis://127.0.0.1:6379/?protocol=3",
            "redis://127.0.0.1:6378",
            "redis://127.0.0.1:6377",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn fail_if_received_different_tls_between_initial_nodes() {
        let result = ClusterClient::new(vec![
            "rediss://127.0.0.1:6379/",
            "redis://127.0.0.1:6378",
            "redis://127.0.0.1:6377",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn give_username_password_by_method() {
        let client = ClusterClientBuilder::new(get_connection_data_with_username_and_password())
            .password("password".to_string())
            .username("user1".to_string())
            .build()
            .unwrap();
        assert_eq!(client.cluster_params.password, Some("password".to_string()));
        assert_eq!(client.cluster_params.username, Some("user1".to_string()));
    }

    #[test]
    fn give_empty_initial_nodes() {
        let client = ClusterClient::new(Vec::<String>::new());
        assert!(client.is_err())
    }
}
