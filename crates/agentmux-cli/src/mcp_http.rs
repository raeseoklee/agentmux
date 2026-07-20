//! Authenticated MCP Streamable HTTP transport for AgentMux server mode.
//!
//! Transport policy stays separate from the MCP tool implementation and is
//! mounted by AgentMux server mode only when explicitly enabled.

use std::{
    collections::{BTreeSet, HashMap},
    convert::Infallible,
    fmt,
    future::Future,
    io,
    net::{IpAddr, SocketAddr},
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, OnceLock, RwLock,
    },
    task::{Context, Poll},
    time::{Duration, Instant, SystemTime},
};

use agentmux_ipc::ControlCaller;
use axum::{body::Body, extract::connect_info::ConnectInfo, Router};
use bytes::Bytes;
use http::{
    header::{AUTHORIZATION, HOST, ORIGIN, WWW_AUTHENTICATE},
    HeaderMap, Method, Request, Response, StatusCode, Uri,
};
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use rmcp::{
    transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    },
    RoleServer,
};
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tower_service::Service;

const SESSION_HEADER: &str = "mcp-session-id";
const PROFILE_HEADER: &str = "x-agentmux-mcp-profile";
const DEFAULT_ENDPOINT: &str = "/mcp";
const DEFAULT_SHUTDOWN_GRACE: Duration = Duration::from_secs(10);
const DEFAULT_SESSION_BINDING_TTL: Duration = Duration::from_secs(30 * 60);
const DEFAULT_MAX_SESSION_BINDINGS: usize = 1_024;

tokio::task_local! {
    static CURRENT_CALLER_HANDLE: String;
}

static NEXT_CALLER_HANDLE: AtomicU64 = AtomicU64::new(1);
static AUTHENTICATED_CALLERS: OnceLock<RwLock<HashMap<String, ControlCaller>>> = OnceLock::new();

fn authenticated_callers() -> &'static RwLock<HashMap<String, ControlCaller>> {
    AUTHENTICATED_CALLERS.get_or_init(|| RwLock::new(HashMap::new()))
}

pub(super) fn current_caller_handle() -> Option<String> {
    CURRENT_CALLER_HANDLE.try_with(Clone::clone).ok()
}

pub(super) fn caller_for_handle(handle: &str) -> Option<ControlCaller> {
    authenticated_callers().read().ok()?.get(handle).cloned()
}

pub(super) fn register_caller(caller: ControlCaller) -> String {
    let sequence = NEXT_CALLER_HANDLE.fetch_add(1, Ordering::Relaxed);
    let handle = format!("mcp-http-{}-{sequence}", std::process::id());
    if let Ok(mut callers) = authenticated_callers().write() {
        callers.insert(handle.clone(), caller);
    }
    handle
}

fn update_caller(handle: &str, caller: ControlCaller) {
    if let Ok(mut callers) = authenticated_callers().write() {
        callers.insert(handle.to_string(), caller);
    }
}

fn remove_caller(handle: &str) {
    if let Ok(mut callers) = authenticated_callers().write() {
        callers.remove(handle);
    }
}

/// Ordered MCP authorization profiles. Higher profiles include lower-profile
/// capabilities, but only when both the server and bearer grant allow them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum McpAccessProfile {
    Read,
    Standard,
    Full,
}

impl McpAccessProfile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Standard => "standard",
            Self::Full => "full",
        }
    }

    pub const fn scope(self) -> &'static str {
        match self {
            Self::Read => "agentmux:mcp:read",
            Self::Standard => "agentmux:mcp:standard",
            Self::Full => "agentmux:mcp:full",
        }
    }

    pub fn parse(value: &str) -> Result<Self, AuthError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "read" | "agentmux:mcp:read" => Ok(Self::Read),
            "standard" | "agentmux:mcp:standard" => Ok(Self::Standard),
            "full" | "agentmux:mcp:full" => Ok(Self::Full),
            _ => Err(AuthError::InvalidProfile),
        }
    }
}

/// An authenticated identity returned by a bearer-token provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenGrant {
    /// Stable, non-secret token identifier used for session binding.
    pub token_id: Arc<str>,
    /// Human or service identity used by audit attribution.
    pub subject: Arc<str>,
    /// Explicit OAuth-style scopes carried by the token.
    pub scopes: BTreeSet<String>,
    /// Hard profile ceiling independent of the scope set.
    pub max_profile: McpAccessProfile,
    pub expires_at: Option<SystemTime>,
}

impl TokenGrant {
    pub fn allows(&self, profile: McpAccessProfile, now: SystemTime) -> bool {
        if self.expires_at.is_some_and(|expires_at| now >= expires_at) || profile > self.max_profile
        {
            return false;
        }

        self.scopes.iter().any(|scope| {
            McpAccessProfile::parse(scope).is_ok_and(|granted_profile| granted_profile >= profile)
        })
    }
}

/// Pluggable bearer validator. Implementations may use the existing AgentMux
/// server token, a local token file, or an external scoped-token store.
pub trait TokenAuthorizer: Send + Sync + 'static {
    fn authorize(&self, bearer: &str, now: SystemTime) -> Result<TokenGrant, AuthError>;
}

/// One exact bearer token and its scope grant.
#[derive(Clone)]
pub struct StaticBearerToken {
    secret: Arc<str>,
    grant: TokenGrant,
}

impl StaticBearerToken {
    pub fn new(
        token_id: impl Into<Arc<str>>,
        subject: impl Into<Arc<str>>,
        secret: impl Into<Arc<str>>,
        max_profile: McpAccessProfile,
    ) -> Result<Self, ConfigError> {
        let secret = secret.into();
        if secret.is_empty() {
            return Err(ConfigError::EmptyBearerSecret);
        }

        let mut scopes = BTreeSet::new();
        scopes.insert(max_profile.scope().to_string());
        Ok(Self {
            secret,
            grant: TokenGrant {
                token_id: token_id.into(),
                subject: subject.into(),
                scopes,
                max_profile,
                expires_at: None,
            },
        })
    }

    #[cfg(test)]
    pub fn with_scopes(mut self, scopes: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.grant.scopes = scopes.into_iter().map(Into::into).collect();
        self
    }

    #[cfg(test)]
    pub fn expires_at(mut self, expires_at: SystemTime) -> Self {
        self.grant.expires_at = Some(expires_at);
        self
    }
}

impl fmt::Debug for StaticBearerToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StaticBearerToken")
            .field("secret", &"<redacted>")
            .field("grant", &self.grant)
            .finish()
    }
}

/// Constant-time static token provider suitable for the existing generated
/// `srv_...` token and small operator-configured token sets.
#[derive(Clone)]
pub struct StaticTokenAuthorizer {
    tokens: Arc<Vec<StaticBearerToken>>,
}

impl StaticTokenAuthorizer {
    pub fn new(tokens: Vec<StaticBearerToken>) -> Result<Self, ConfigError> {
        if tokens.is_empty() {
            return Err(ConfigError::NoBearerTokens);
        }
        let mut ids = BTreeSet::new();
        for (index, token) in tokens.iter().enumerate() {
            if !ids.insert(token.grant.token_id.to_string()) {
                return Err(ConfigError::DuplicateTokenId(
                    token.grant.token_id.to_string(),
                ));
            }
            if tokens[..index].iter().any(|existing| {
                constant_time_eq(existing.secret.as_bytes(), token.secret.as_bytes())
            }) {
                return Err(ConfigError::DuplicateBearerSecret);
            }
        }
        Ok(Self {
            tokens: Arc::new(tokens),
        })
    }
}

impl fmt::Debug for StaticTokenAuthorizer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StaticTokenAuthorizer")
            .field("token_count", &self.tokens.len())
            .finish()
    }
}

impl TokenAuthorizer for StaticTokenAuthorizer {
    fn authorize(&self, bearer: &str, now: SystemTime) -> Result<TokenGrant, AuthError> {
        let mut match_index = None;
        for (index, candidate) in self.tokens.iter().enumerate() {
            if constant_time_eq(bearer.as_bytes(), candidate.secret.as_bytes()) {
                match_index = Some(index);
            }
        }
        let grant = match_index
            .and_then(|index| self.tokens.get(index))
            .map(|token| token.grant.clone())
            .ok_or(AuthError::InvalidToken)?;

        if grant.expires_at.is_some_and(|expires_at| now >= expires_at) {
            return Err(AuthError::ExpiredToken);
        }
        Ok(grant)
    }
}

#[derive(Debug, Clone)]
pub struct McpHttpConfig {
    pub bind_addr: SocketAddr,
    pub endpoint_path: String,
    pub allowed_hosts: Vec<String>,
    pub allowed_origins: Vec<String>,
    pub default_profile: McpAccessProfile,
    pub max_profile: McpAccessProfile,
    pub allow_insecure_remote_http: bool,
    pub shutdown_grace: Duration,
    pub sse_keep_alive: Option<Duration>,
    pub sse_retry: Option<Duration>,
    pub session_binding_ttl: Duration,
    pub max_session_bindings: usize,
}

impl McpHttpConfig {
    pub fn new(bind_addr: SocketAddr) -> Self {
        Self {
            bind_addr,
            endpoint_path: DEFAULT_ENDPOINT.to_string(),
            allowed_hosts: Vec::new(),
            allowed_origins: Vec::new(),
            default_profile: McpAccessProfile::Read,
            max_profile: McpAccessProfile::Read,
            allow_insecure_remote_http: false,
            shutdown_grace: DEFAULT_SHUTDOWN_GRACE,
            sse_keep_alive: Some(Duration::from_secs(15)),
            sse_retry: Some(Duration::from_secs(3)),
            session_binding_ttl: DEFAULT_SESSION_BINDING_TTL,
            max_session_bindings: DEFAULT_MAX_SESSION_BINDINGS,
        }
    }

    #[cfg(test)]
    pub fn loopback(port: u16) -> Self {
        Self::new(SocketAddr::from(([127, 0, 0, 1], port)))
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if !self.endpoint_path.starts_with('/')
            || self.endpoint_path.contains('?')
            || self.endpoint_path.contains('#')
        {
            return Err(ConfigError::InvalidEndpointPath(self.endpoint_path.clone()));
        }
        if self.default_profile > self.max_profile {
            return Err(ConfigError::DefaultProfileExceedsCeiling);
        }
        if self.session_binding_ttl.is_zero() {
            return Err(ConfigError::InvalidSessionBindingTtl);
        }
        if self.max_session_bindings == 0 {
            return Err(ConfigError::InvalidMaxSessionBindings);
        }
        if !self.bind_addr.ip().is_loopback() && self.allowed_hosts.is_empty() {
            return Err(ConfigError::RemoteBindRequiresAllowedHosts);
        }
        if !self.bind_addr.ip().is_loopback() && !self.allow_insecure_remote_http {
            return Err(ConfigError::RemoteBindRequiresInsecureOverride);
        }
        if self
            .allowed_hosts
            .iter()
            .any(|host| host.trim().is_empty() || host.contains('*'))
        {
            return Err(ConfigError::UnsafeAllowedHost);
        }
        if self.allowed_origins.iter().any(|origin| {
            origin.trim().is_empty()
                || origin.contains('*')
                || (origin != "null" && parse_origin(origin).is_none())
        }) {
            return Err(ConfigError::UnsafeAllowedOrigin);
        }
        Ok(())
    }

    fn resolve_for_bound_addr(mut self, bound_addr: SocketAddr) -> Result<Self, ConfigError> {
        self.bind_addr = bound_addr;
        if self.allowed_hosts.is_empty() {
            if !bound_addr.ip().is_loopback() {
                return Err(ConfigError::RemoteBindRequiresAllowedHosts);
            }
            self.allowed_hosts = loopback_allowed_hosts(bound_addr);
        }
        self.validate()?;
        Ok(self)
    }

    fn rmcp_config(&self, cancellation_token: CancellationToken) -> StreamableHttpServerConfig {
        StreamableHttpServerConfig::default()
            .with_stateful_mode(true)
            .with_json_response(false)
            .with_sse_keep_alive(self.sse_keep_alive)
            .with_sse_retry(self.sse_retry)
            .with_allowed_hosts(self.allowed_hosts.clone())
            .with_allowed_origins(self.allowed_origins.clone())
            .with_cancellation_token(cancellation_token)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    EmptyBearerSecret,
    NoBearerTokens,
    DuplicateTokenId(String),
    DuplicateBearerSecret,
    InvalidEndpointPath(String),
    DefaultProfileExceedsCeiling,
    RemoteBindRequiresAllowedHosts,
    RemoteBindRequiresInsecureOverride,
    UnsafeAllowedHost,
    UnsafeAllowedOrigin,
    DynamicPortMustBeResolved,
    InvalidSessionBindingTtl,
    InvalidMaxSessionBindings,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyBearerSecret => formatter.write_str("bearer token secret must not be empty"),
            Self::NoBearerTokens => formatter.write_str("at least one bearer token is required"),
            Self::DuplicateTokenId(id) => write!(formatter, "duplicate bearer token id '{id}'"),
            Self::DuplicateBearerSecret => {
                formatter.write_str("bearer token secrets must be unique")
            }
            Self::InvalidEndpointPath(path) => write!(
                formatter,
                "MCP endpoint path '{path}' must be an absolute path without query or fragment"
            ),
            Self::DefaultProfileExceedsCeiling => {
                formatter.write_str("default MCP profile exceeds the server profile ceiling")
            }
            Self::RemoteBindRequiresAllowedHosts => {
                formatter.write_str("non-loopback MCP binds require an explicit allowed_hosts list")
            }
            Self::RemoteBindRequiresInsecureOverride => formatter.write_str(
                "non-loopback MCP bearer authentication over plain HTTP requires an explicit insecure-remote override",
            ),
            Self::UnsafeAllowedHost => {
                formatter.write_str("allowed_hosts entries must be exact and non-empty")
            }
            Self::UnsafeAllowedOrigin => formatter
                .write_str("allowed_origins entries must be exact RFC 6454 origins or 'null'"),
            Self::DynamicPortMustBeResolved => formatter.write_str(
                "an MCP HTTP service created without binding cannot use dynamic port zero",
            ),
            Self::InvalidSessionBindingTtl => {
                formatter.write_str("MCP HTTP session binding TTL must be greater than zero")
            }
            Self::InvalidMaxSessionBindings => {
                formatter.write_str("MCP HTTP max session bindings must be greater than zero")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthError {
    MissingAuthorization,
    MalformedAuthorization,
    InvalidToken,
    ExpiredToken,
    InvalidProfile,
    InsufficientScope,
    ProfileExceedsServerCeiling,
    SessionNotBound,
    SessionPrincipalMismatch,
    SessionProfileMismatch,
}

/// Request identity exposed through `RequestContext` HTTP extensions.
#[derive(Debug, Clone)]
#[allow(dead_code)] // rmcp consumes this request extension inside its service boundary.
pub struct RemoteMcpContext {
    pub token_id: Arc<str>,
    pub subject: Arc<str>,
    pub profile: McpAccessProfile,
    pub session_id: Option<Arc<str>>,
    pub peer_addr: Option<SocketAddr>,
}

impl RemoteMcpContext {
    fn to_control_caller(&self) -> ControlCaller {
        let peer = self
            .peer_addr
            .map(|address| address.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        ControlCaller {
            source: format!(
                "mcp-http;token_id={};subject={};peer={}",
                audit_component(&self.token_id),
                audit_component(&self.subject),
                audit_component(&peer),
            ),
            profile: Some(self.profile.as_str().to_string()),
            client_session_id: self.session_id.as_deref().map(ToOwned::to_owned),
        }
    }
}

fn audit_component(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace('=', "\\=")
}

#[derive(Debug, Clone)]
struct SessionBinding {
    token_id: Arc<str>,
    subject: Arc<str>,
    profile: McpAccessProfile,
    caller_handle: String,
    last_seen: Instant,
}

struct SessionBindings {
    entries: tokio::sync::RwLock<HashMap<String, SessionBinding>>,
    ttl: Duration,
    max_entries: usize,
}

impl SessionBindings {
    fn new(ttl: Duration, max_entries: usize) -> Self {
        Self {
            entries: tokio::sync::RwLock::new(HashMap::new()),
            ttl,
            max_entries,
        }
    }

    async fn get(&self, session_id: &str) -> Option<SessionBinding> {
        self.get_at(session_id, Instant::now()).await
    }

    async fn get_at(&self, session_id: &str, now: Instant) -> Option<SessionBinding> {
        let mut entries = self.entries.write().await;
        Self::prune_expired(&mut entries, now, self.ttl);
        let binding = entries.get_mut(session_id)?;
        binding.last_seen = now;
        Some(binding.clone())
    }

    async fn bind(&self, session_id: String, binding: SessionBinding) -> Result<(), AuthError> {
        self.bind_at(session_id, binding, Instant::now()).await
    }

    async fn bind_at(
        &self,
        session_id: String,
        mut binding: SessionBinding,
        now: Instant,
    ) -> Result<(), AuthError> {
        let mut entries = self.entries.write().await;
        Self::prune_expired(&mut entries, now, self.ttl);
        if let Some(existing) = entries.get(&session_id) {
            if existing.token_id != binding.token_id || existing.subject != binding.subject {
                return Err(AuthError::SessionPrincipalMismatch);
            }
            if existing.profile != binding.profile {
                return Err(AuthError::SessionProfileMismatch);
            }
            return Ok(());
        }
        if entries.len() >= self.max_entries {
            if let Some(oldest_id) = entries
                .iter()
                .min_by_key(|(_, existing)| existing.last_seen)
                .map(|(id, _)| id.clone())
            {
                if let Some(evicted) = entries.remove(&oldest_id) {
                    remove_caller(&evicted.caller_handle);
                }
            }
        }
        binding.last_seen = now;
        entries.insert(session_id, binding);
        Ok(())
    }

    async fn remove(&self, session_id: &str) {
        if let Some(binding) = self.entries.write().await.remove(session_id) {
            remove_caller(&binding.caller_handle);
        }
    }

    fn prune_expired(entries: &mut HashMap<String, SessionBinding>, now: Instant, ttl: Duration) {
        let expired_handles = entries
            .extract_if(|_, binding| now.saturating_duration_since(binding.last_seen) >= ttl)
            .map(|(_, binding)| binding.caller_handle)
            .collect::<Vec<_>>();
        for handle in expired_handles {
            remove_caller(&handle);
        }
    }
}

struct ProfileServices<S> {
    read: StreamableHttpService<S, LocalSessionManager>,
    standard: StreamableHttpService<S, LocalSessionManager>,
    full: StreamableHttpService<S, LocalSessionManager>,
}

impl<S> Clone for ProfileServices<S> {
    fn clone(&self) -> Self {
        Self {
            read: self.read.clone(),
            standard: self.standard.clone(),
            full: self.full.clone(),
        }
    }
}

impl<S> ProfileServices<S>
where
    S: rmcp::Service<RoleServer> + Send + 'static,
{
    fn new<F>(factory: Arc<F>, config: StreamableHttpServerConfig) -> Self
    where
        F: Fn(McpAccessProfile) -> Result<S, io::Error> + Send + Sync + 'static,
    {
        let sessions = Arc::new(LocalSessionManager::default());
        let service_for = |profile| {
            let factory = Arc::clone(&factory);
            StreamableHttpService::new(
                move || factory(profile),
                Arc::clone(&sessions),
                config.clone(),
            )
        };
        Self {
            read: service_for(McpAccessProfile::Read),
            standard: service_for(McpAccessProfile::Standard),
            full: service_for(McpAccessProfile::Full),
        }
    }

    async fn handle(&self, profile: McpAccessProfile, request: Request<Body>) -> McpResponse {
        match profile {
            McpAccessProfile::Read => self.read.handle(request).await,
            McpAccessProfile::Standard => self.standard.handle(request).await,
            McpAccessProfile::Full => self.full.handle(request).await,
        }
    }
}

type McpResponse = Response<BoxBody<Bytes, Infallible>>;

struct McpHttpInner<S> {
    config: McpHttpConfig,
    authorizer: Arc<dyn TokenAuthorizer>,
    services: ProfileServices<S>,
    bindings: SessionBindings,
    cancellation_token: CancellationToken,
}

/// Tower/axum service that authenticates before delegating to rmcp 2.2's
/// stateful Streamable HTTP implementation.
pub struct AgentMuxMcpHttpService<S> {
    inner: Arc<McpHttpInner<S>>,
}

impl<S> Clone for AgentMuxMcpHttpService<S> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<S> AgentMuxMcpHttpService<S>
where
    S: rmcp::Service<RoleServer> + Send + 'static,
{
    pub fn new<F>(
        config: McpHttpConfig,
        authorizer: Arc<dyn TokenAuthorizer>,
        factory: F,
        cancellation_token: CancellationToken,
    ) -> Result<Self, ConfigError>
    where
        F: Fn(McpAccessProfile) -> Result<S, io::Error> + Send + Sync + 'static,
    {
        let config = if config.allowed_hosts.is_empty() {
            if config.bind_addr.port() == 0 {
                return Err(ConfigError::DynamicPortMustBeResolved);
            }
            let bound_addr = config.bind_addr;
            config.resolve_for_bound_addr(bound_addr)?
        } else {
            config.validate()?;
            config
        };
        let services = ProfileServices::new(
            Arc::new(factory),
            config.rmcp_config(cancellation_token.child_token()),
        );
        let bindings =
            SessionBindings::new(config.session_binding_ttl, config.max_session_bindings);
        Ok(Self {
            inner: Arc::new(McpHttpInner {
                config,
                authorizer,
                services,
                bindings,
                cancellation_token,
            }),
        })
    }

    async fn handle(&self, mut request: Request<Body>) -> McpResponse {
        if self.inner.cancellation_token.is_cancelled() {
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "server_shutting_down",
                None,
            );
        }
        if let Err(rejection) = validate_request_target(&request, &self.inner.config) {
            return rejection.into_response();
        }

        let now = SystemTime::now();
        let bearer = match bearer_from_headers(request.headers()) {
            Ok(bearer) => bearer,
            Err(error) => return auth_error_response(error, None),
        };
        let grant = match self.inner.authorizer.authorize(bearer, now) {
            Ok(grant) => grant,
            Err(error) => return auth_error_response(error, None),
        };

        let requested_profile = match requested_profile(request.headers()) {
            Ok(profile) => profile,
            Err(error) => return auth_error_response(error, None),
        };
        let inbound_session_id = match single_header(request.headers(), SESSION_HEADER) {
            Ok(session_id) => session_id.map(str::to_owned),
            Err(()) => {
                return error_response(StatusCode::BAD_REQUEST, "invalid_session_header", None);
            }
        };

        let existing_binding = if let Some(session_id) = inbound_session_id.as_deref() {
            let Some(binding) = self.inner.bindings.get(session_id).await else {
                return auth_error_response(AuthError::SessionNotBound, None);
            };
            if binding.token_id != grant.token_id || binding.subject != grant.subject {
                return auth_error_response(AuthError::SessionPrincipalMismatch, None);
            }
            if requested_profile.is_some_and(|requested| requested != binding.profile) {
                return auth_error_response(AuthError::SessionProfileMismatch, None);
            }
            Some(binding)
        } else {
            None
        };
        let profile = existing_binding
            .as_ref()
            .map(|binding| binding.profile)
            .unwrap_or_else(|| requested_profile.unwrap_or(self.inner.config.default_profile));

        if profile > self.inner.config.max_profile {
            return auth_error_response(AuthError::ProfileExceedsServerCeiling, Some(profile));
        }
        if !grant.allows(profile, now) {
            return auth_error_response(AuthError::InsufficientScope, Some(profile));
        }

        let peer_addr = request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|connect_info| connect_info.0);
        let remote_context = RemoteMcpContext {
            token_id: Arc::clone(&grant.token_id),
            subject: Arc::clone(&grant.subject),
            profile,
            session_id: inbound_session_id.clone().map(Arc::from),
            peer_addr,
        };
        let caller_handle = existing_binding
            .as_ref()
            .map(|binding| binding.caller_handle.clone())
            .unwrap_or_else(|| register_caller(remote_context.to_control_caller()));
        update_caller(&caller_handle, remote_context.to_control_caller());
        request.extensions_mut().insert(remote_context);

        let method = request.method().clone();
        let mut response = CURRENT_CALLER_HANDLE
            .scope(
                caller_handle.clone(),
                self.inner.services.handle(profile, request),
            )
            .await;

        if inbound_session_id.is_none() {
            if let Ok(Some(session_id)) = single_header(response.headers(), SESSION_HEADER) {
                let caller = RemoteMcpContext {
                    token_id: Arc::clone(&grant.token_id),
                    subject: Arc::clone(&grant.subject),
                    profile,
                    session_id: Some(Arc::from(session_id)),
                    peer_addr,
                }
                .to_control_caller();
                update_caller(&caller_handle, caller);
                let binding = SessionBinding {
                    token_id: Arc::clone(&grant.token_id),
                    subject: Arc::clone(&grant.subject),
                    profile,
                    caller_handle: caller_handle.clone(),
                    last_seen: Instant::now(),
                };
                if let Err(error) = self
                    .inner
                    .bindings
                    .bind(session_id.to_string(), binding)
                    .await
                {
                    remove_caller(&caller_handle);
                    return auth_error_response(error, Some(profile));
                }
            } else {
                remove_caller(&caller_handle);
            }
        }

        if let Some(session_id) = inbound_session_id.as_deref() {
            let session_closed = method == Method::DELETE && response.status().is_success();
            let session_expired =
                matches!(response.status(), StatusCode::NOT_FOUND | StatusCode::GONE);
            if session_closed || session_expired {
                self.inner.bindings.remove(session_id).await;
            }
        }

        response
            .headers_mut()
            .insert("cache-control", http::HeaderValue::from_static("no-store"));
        response.headers_mut().insert(
            "x-content-type-options",
            http::HeaderValue::from_static("nosniff"),
        );
        response
    }
}

impl<S> Service<Request<Body>> for AgentMuxMcpHttpService<S>
where
    S: rmcp::Service<RoleServer> + Send + 'static,
{
    type Response = McpResponse;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let service = self.clone();
        Box::pin(async move { Ok(service.handle(request).await) })
    }
}

pub struct RunningMcpHttpServer {
    local_addr: SocketAddr,
    endpoint_path: String,
    cancellation_token: CancellationToken,
    shutdown_grace: Duration,
    join: Option<JoinHandle<io::Result<()>>>,
}

impl RunningMcpHttpServer {
    #[cfg(test)]
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub fn endpoint_url(&self) -> String {
        let host = match self.local_addr.ip() {
            IpAddr::V4(ip) => ip.to_string(),
            IpAddr::V6(ip) => format!("[{ip}]"),
        };
        format!(
            "http://{host}:{}{}",
            self.local_addr.port(),
            self.endpoint_path
        )
    }

    pub async fn shutdown(mut self) -> io::Result<()> {
        self.cancellation_token.cancel();
        let mut join = self.join.take().expect("server join handle must exist");
        match tokio::time::timeout(self.shutdown_grace, &mut join).await {
            Ok(result) => flatten_join(result),
            Err(_) => {
                join.abort();
                let _ = join.await;
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "MCP HTTP server exceeded its graceful shutdown deadline",
                ))
            }
        }
    }
}

impl Drop for RunningMcpHttpServer {
    fn drop(&mut self) {
        self.cancellation_token.cancel();
    }
}

/// Bind the explicit address, build three profile-specific rmcp services, and
/// run them behind the authenticated session-binding gate.
pub async fn spawn_mcp_http_server<S, F>(
    config: McpHttpConfig,
    authorizer: Arc<dyn TokenAuthorizer>,
    factory: F,
) -> Result<RunningMcpHttpServer, io::Error>
where
    S: rmcp::Service<RoleServer> + Send + 'static,
    F: Fn(McpAccessProfile) -> Result<S, io::Error> + Send + Sync + 'static,
{
    config
        .validate()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let listener = TcpListener::bind(config.bind_addr).await?;
    let local_addr = listener.local_addr()?;
    let config = config
        .resolve_for_bound_addr(local_addr)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let cancellation_token = CancellationToken::new();
    let service = AgentMuxMcpHttpService::new(
        config.clone(),
        authorizer,
        factory,
        cancellation_token.child_token(),
    )
    .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;

    let app = Router::new().route_service(&config.endpoint_path, service);
    let shutdown = cancellation_token.clone();
    let join = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown.cancelled_owned())
        .await
        .map_err(io::Error::other)
    });

    Ok(RunningMcpHttpServer {
        local_addr,
        endpoint_path: config.endpoint_path,
        cancellation_token,
        shutdown_grace: config.shutdown_grace,
        join: Some(join),
    })
}

fn flatten_join(result: Result<io::Result<()>, tokio::task::JoinError>) -> io::Result<()> {
    result.map_err(|error| io::Error::other(format!("MCP HTTP server task failed: {error}")))?
}

fn bearer_from_headers(headers: &HeaderMap) -> Result<&str, AuthError> {
    let value = single_header(headers, AUTHORIZATION.as_str())
        .map_err(|()| AuthError::MalformedAuthorization)?
        .ok_or(AuthError::MissingAuthorization)?;
    let (scheme, token) = value
        .split_once(' ')
        .ok_or(AuthError::MalformedAuthorization)?;
    if !scheme.eq_ignore_ascii_case("bearer")
        || token.is_empty()
        || token.trim() != token
        || token.chars().any(char::is_whitespace)
    {
        return Err(AuthError::MalformedAuthorization);
    }
    Ok(token)
}

fn requested_profile(headers: &HeaderMap) -> Result<Option<McpAccessProfile>, AuthError> {
    single_header(headers, PROFILE_HEADER)
        .map_err(|()| AuthError::InvalidProfile)?
        .map(McpAccessProfile::parse)
        .transpose()
}

fn single_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, ()> {
    let mut values = headers.get_all(name).iter();
    let Some(first) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(());
    }
    first.to_str().map(Some).map_err(|_| ())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RequestRejection {
    status: StatusCode,
    code: &'static str,
}

impl RequestRejection {
    const fn new(status: StatusCode, code: &'static str) -> Self {
        Self { status, code }
    }

    fn into_response(self) -> McpResponse {
        error_response(self.status, self.code, None)
    }
}

fn validate_request_target<S>(
    request: &Request<S>,
    config: &McpHttpConfig,
) -> Result<(), RequestRejection> {
    let authority = request_authority(request.uri(), request.headers())?;
    if !config
        .allowed_hosts
        .iter()
        .filter_map(|allowed| parse_authority(allowed))
        .any(|allowed| authority_matches(&authority, &allowed))
    {
        return Err(RequestRejection::new(
            StatusCode::FORBIDDEN,
            "host_not_allowed",
        ));
    }

    let origin = single_header(request.headers(), ORIGIN.as_str())
        .map_err(|()| RequestRejection::new(StatusCode::BAD_REQUEST, "invalid_origin_header"))?;
    if let Some(origin) = origin {
        let Some(origin) = parse_origin(origin) else {
            return Err(RequestRejection::new(
                StatusCode::BAD_REQUEST,
                "invalid_origin_header",
            ));
        };
        if config.allowed_origins.is_empty()
            || !config
                .allowed_origins
                .iter()
                .filter_map(|allowed| parse_origin(allowed))
                .any(|allowed| origin_matches(&origin, &allowed))
        {
            return Err(RequestRejection::new(
                StatusCode::FORBIDDEN,
                "origin_not_allowed",
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Authority {
    host: String,
    port: Option<u16>,
}

fn request_authority(uri: &Uri, headers: &HeaderMap) -> Result<Authority, RequestRejection> {
    let raw = single_header(headers, HOST.as_str())
        .map_err(|()| RequestRejection::new(StatusCode::BAD_REQUEST, "invalid_host_header"))?
        .or_else(|| uri.authority().map(|authority| authority.as_str()))
        .ok_or_else(|| RequestRejection::new(StatusCode::BAD_REQUEST, "missing_host_header"))?;
    parse_authority(raw)
        .ok_or_else(|| RequestRejection::new(StatusCode::BAD_REQUEST, "invalid_host_header"))
}

fn parse_authority(raw: &str) -> Option<Authority> {
    let authority = http::uri::Authority::try_from(raw.trim()).ok()?;
    Some(Authority {
        host: authority
            .host()
            .trim_matches(['[', ']'])
            .to_ascii_lowercase(),
        port: authority.port_u16(),
    })
}

fn authority_matches(actual: &Authority, allowed: &Authority) -> bool {
    actual.host == allowed.host && allowed.port.is_none_or(|port| actual.port == Some(port))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Origin {
    Null,
    Tuple {
        scheme: String,
        host: String,
        port: Option<u16>,
    },
}

fn parse_origin(raw: &str) -> Option<Origin> {
    let raw = raw.trim();
    if raw == "null" {
        return Some(Origin::Null);
    }
    let uri = Uri::try_from(raw).ok()?;
    if uri
        .path_and_query()
        .is_some_and(|path| path.as_str() != "/")
    {
        return None;
    }
    let scheme = uri.scheme_str()?.to_ascii_lowercase();
    if !matches!(scheme.as_str(), "http" | "https") {
        return None;
    }
    let authority = uri.authority()?;
    Some(Origin::Tuple {
        scheme,
        host: authority
            .host()
            .trim_matches(['[', ']'])
            .to_ascii_lowercase(),
        port: authority.port_u16(),
    })
}

fn origin_matches(actual: &Origin, allowed: &Origin) -> bool {
    match (actual, allowed) {
        (Origin::Null, Origin::Null) => true,
        (
            Origin::Tuple {
                scheme: actual_scheme,
                host: actual_host,
                port: actual_port,
            },
            Origin::Tuple {
                scheme: allowed_scheme,
                host: allowed_host,
                port: allowed_port,
            },
        ) => {
            actual_scheme == allowed_scheme
                && actual_host == allowed_host
                && actual_port == allowed_port
        }
        _ => false,
    }
}

fn loopback_allowed_hosts(addr: SocketAddr) -> Vec<String> {
    let port = addr.port();
    match addr.ip() {
        IpAddr::V4(ip) => {
            let mut hosts = vec![format!("{ip}:{port}")];
            if ip.is_loopback() {
                hosts.push(format!("localhost:{port}"));
            }
            hosts
        }
        IpAddr::V6(ip) => {
            let mut hosts = vec![format!("[{ip}]:{port}")];
            if ip.is_loopback() {
                hosts.push(format!("localhost:{port}"));
            }
            hosts
        }
    }
}

fn auth_error_response(error: AuthError, profile: Option<McpAccessProfile>) -> McpResponse {
    let (status, code) = match error {
        AuthError::MissingAuthorization => (StatusCode::UNAUTHORIZED, "missing_bearer_token"),
        AuthError::MalformedAuthorization => (StatusCode::UNAUTHORIZED, "malformed_bearer_token"),
        AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, "invalid_bearer_token"),
        AuthError::ExpiredToken => (StatusCode::UNAUTHORIZED, "expired_bearer_token"),
        AuthError::InvalidProfile => (StatusCode::BAD_REQUEST, "invalid_mcp_profile"),
        AuthError::InsufficientScope => (StatusCode::FORBIDDEN, "insufficient_scope"),
        AuthError::ProfileExceedsServerCeiling => {
            (StatusCode::FORBIDDEN, "profile_exceeds_server_ceiling")
        }
        AuthError::SessionNotBound => (StatusCode::UNAUTHORIZED, "unbound_mcp_session"),
        AuthError::SessionPrincipalMismatch => {
            (StatusCode::FORBIDDEN, "mcp_session_principal_mismatch")
        }
        AuthError::SessionProfileMismatch => (StatusCode::CONFLICT, "mcp_session_profile_mismatch"),
    };
    let challenge = profile.map(|profile| profile.scope());
    error_response(status, code, challenge)
}

fn error_response(status: StatusCode, code: &str, required_scope: Option<&str>) -> McpResponse {
    let body = serde_json::json!({ "error": code }).to_string();
    let mut response = Response::builder()
        .status(status)
        .header("content-type", "application/json; charset=utf-8")
        .header("cache-control", "no-store");
    if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
        let challenge = match required_scope {
            Some(scope) => format!("Bearer realm=\"agentmux-mcp\", scope=\"{scope}\""),
            None => "Bearer realm=\"agentmux-mcp\"".to_string(),
        };
        if let Ok(value) = http::HeaderValue::from_str(&challenge) {
            response = response.header(WWW_AUTHENTICATE, value);
        }
    }
    response
        .body(Full::new(Bytes::from(body)).boxed())
        .expect("static MCP error response must be valid")
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let max_len = left.len().max(right.len());
    let mut difference = left.len() ^ right.len();
    for index in 0..max_len {
        let left_byte = left.get(index).copied().unwrap_or_default();
        let right_byte = right.get(index).copied().unwrap_or_default();
        difference |= usize::from(left_byte ^ right_byte);
    }
    difference == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::{
        handler::server::ServerHandler,
        model::{ServerCapabilities, ServerInfo},
    };

    #[derive(Clone)]
    struct TestHandler;

    impl ServerHandler for TestHandler {
        fn get_info(&self) -> ServerInfo {
            ServerInfo::new(ServerCapabilities::builder().build())
        }
    }

    fn token(id: &str, secret: &str, profile: McpAccessProfile) -> StaticBearerToken {
        StaticBearerToken::new(id, format!("subject-{id}"), secret, profile).unwrap()
    }

    fn request(host: &str, origin: Option<&str>) -> Request<()> {
        let mut builder = Request::builder().uri("/mcp").header(HOST, host);
        if let Some(origin) = origin {
            builder = builder.header(ORIGIN, origin);
        }
        builder.body(()).unwrap()
    }

    #[test]
    fn profile_order_and_scopes_form_a_strict_ceiling() {
        assert!(McpAccessProfile::Read < McpAccessProfile::Standard);
        assert!(McpAccessProfile::Standard < McpAccessProfile::Full);
        assert_eq!(
            McpAccessProfile::parse("agentmux:mcp:standard"),
            Ok(McpAccessProfile::Standard)
        );
        assert_eq!(
            McpAccessProfile::parse("administrator"),
            Err(AuthError::InvalidProfile)
        );
    }

    #[test]
    fn static_authorizer_matches_exact_tokens_without_exposing_secrets() {
        let authorizer = StaticTokenAuthorizer::new(vec![
            token("read", "srv_read", McpAccessProfile::Read),
            token("standard", "srv_standard", McpAccessProfile::Standard),
        ])
        .unwrap();

        let grant = authorizer
            .authorize("srv_standard", SystemTime::now())
            .unwrap();
        assert_eq!(&*grant.token_id, "standard");
        assert!(grant.allows(McpAccessProfile::Read, SystemTime::now()));
        assert!(grant.allows(McpAccessProfile::Standard, SystemTime::now()));
        assert!(!grant.allows(McpAccessProfile::Full, SystemTime::now()));
        assert_eq!(
            authorizer.authorize("srv_standar", SystemTime::now()),
            Err(AuthError::InvalidToken)
        );
        assert!(!format!("{authorizer:?}").contains("srv_standard"));
    }

    #[test]
    fn static_authorizer_rejects_ambiguous_ids_and_secrets() {
        assert_eq!(
            StaticTokenAuthorizer::new(vec![
                token("same", "srv_one", McpAccessProfile::Read),
                token("same", "srv_two", McpAccessProfile::Read),
            ])
            .unwrap_err(),
            ConfigError::DuplicateTokenId("same".to_string())
        );
        assert_eq!(
            StaticTokenAuthorizer::new(vec![
                token("one", "srv_same", McpAccessProfile::Read),
                token("two", "srv_same", McpAccessProfile::Read),
            ])
            .unwrap_err(),
            ConfigError::DuplicateBearerSecret
        );
    }

    #[test]
    fn expired_and_missing_scope_tokens_are_rejected() {
        let expired = token("expired", "srv_expired", McpAccessProfile::Full)
            .expires_at(SystemTime::UNIX_EPOCH);
        let authorizer = StaticTokenAuthorizer::new(vec![expired]).unwrap();
        assert_eq!(
            authorizer.authorize("srv_expired", SystemTime::now()),
            Err(AuthError::ExpiredToken)
        );

        let grant = token("scoped", "srv_scoped", McpAccessProfile::Full)
            .with_scopes([McpAccessProfile::Read.scope()])
            .grant;
        assert!(!grant.allows(McpAccessProfile::Standard, SystemTime::now()));
    }

    #[test]
    fn bearer_header_parser_is_strict() {
        let valid = HeaderMap::from_iter([(
            AUTHORIZATION,
            http::HeaderValue::from_static("Bearer srv_secret"),
        )]);
        assert_eq!(bearer_from_headers(&valid), Ok("srv_secret"));

        for invalid in ["srv_secret", "Basic abc", "Bearer ", "Bearer one two"] {
            let mut headers = HeaderMap::new();
            headers.insert(AUTHORIZATION, http::HeaderValue::from_str(invalid).unwrap());
            assert_eq!(
                bearer_from_headers(&headers),
                Err(AuthError::MalformedAuthorization)
            );
        }
        assert_eq!(
            bearer_from_headers(&HeaderMap::new()),
            Err(AuthError::MissingAuthorization)
        );
    }

    #[test]
    fn remote_bind_requires_an_exact_host_allowlist() {
        let mut config = McpHttpConfig::loopback(8765);
        config.bind_addr = "0.0.0.0:8765".parse().unwrap();
        assert_eq!(
            config.validate(),
            Err(ConfigError::RemoteBindRequiresAllowedHosts)
        );
        config.allowed_hosts = vec!["mcp.example.test:8765".to_string()];
        assert_eq!(
            config.validate(),
            Err(ConfigError::RemoteBindRequiresInsecureOverride)
        );
        config.allow_insecure_remote_http = true;
        assert!(config.validate().is_ok());
        config.allowed_hosts = vec!["*.example.test".to_string()];
        assert_eq!(config.validate(), Err(ConfigError::UnsafeAllowedHost));
    }

    #[test]
    fn bound_loopback_port_is_pinned_in_allowed_hosts() {
        let config = McpHttpConfig::loopback(0)
            .resolve_for_bound_addr("127.0.0.1:43127".parse().unwrap())
            .unwrap();
        assert_eq!(
            config.allowed_hosts,
            vec!["127.0.0.1:43127", "localhost:43127"]
        );
        assert!(validate_request_target(&request("localhost:43127", None), &config).is_ok());
        assert!(validate_request_target(&request("localhost:43128", None), &config).is_err());
    }

    #[test]
    fn origin_is_rejected_unless_explicitly_allowed() {
        let mut config = McpHttpConfig::loopback(43127)
            .resolve_for_bound_addr("127.0.0.1:43127".parse().unwrap())
            .unwrap();
        assert!(validate_request_target(
            &request("127.0.0.1:43127", Some("http://evil.example")),
            &config,
        )
        .is_err());
        config.allowed_origins = vec!["https://agent.example".to_string()];
        assert!(validate_request_target(
            &request("127.0.0.1:43127", Some("https://agent.example")),
            &config,
        )
        .is_ok());
        assert!(validate_request_target(
            &request("127.0.0.1:43127", Some("https://agent.example:444")),
            &config,
        )
        .is_err());
    }

    #[test]
    fn ipv6_authorities_are_normalized_without_losing_the_port() {
        assert_eq!(
            parse_authority("[::1]:8765"),
            Some(Authority {
                host: "::1".to_string(),
                port: Some(8765),
            })
        );
        assert_eq!(
            loopback_allowed_hosts("[::1]:8765".parse().unwrap()),
            vec!["[::1]:8765", "localhost:8765"]
        );
    }

    #[tokio::test]
    async fn session_binding_cannot_change_principal_or_profile() {
        let bindings = SessionBindings::new(Duration::from_secs(60), 8);
        let now = Instant::now();
        let original = SessionBinding {
            token_id: Arc::from("token-a"),
            subject: Arc::from("agent-a"),
            profile: McpAccessProfile::Standard,
            caller_handle: register_caller(ControlCaller {
                source: "mcp-http;token_id=token-a;subject=agent-a;peer=127.0.0.1:1".to_string(),
                profile: Some("standard".to_string()),
                client_session_id: None,
            }),
            last_seen: now,
        };
        bindings
            .bind_at("session-1".to_string(), original.clone(), now)
            .await
            .unwrap();
        assert_eq!(
            bindings.get("session-1").await.unwrap().subject,
            original.subject
        );

        let other_principal = SessionBinding {
            token_id: Arc::from("token-b"),
            ..original.clone()
        };
        assert_eq!(
            bindings
                .bind("session-1".to_string(), other_principal)
                .await,
            Err(AuthError::SessionPrincipalMismatch)
        );

        let other_profile = SessionBinding {
            profile: McpAccessProfile::Full,
            ..original
        };
        assert_eq!(
            bindings.bind("session-1".to_string(), other_profile).await,
            Err(AuthError::SessionProfileMismatch)
        );
    }

    #[tokio::test]
    async fn session_bindings_expire_and_evict_the_oldest_entry() {
        let bindings = SessionBindings::new(Duration::from_secs(10), 2);
        let start = Instant::now();
        let binding = |id: &str, seen: Instant| SessionBinding {
            token_id: Arc::from(id),
            subject: Arc::from(format!("subject-{id}")),
            profile: McpAccessProfile::Read,
            caller_handle: register_caller(ControlCaller {
                source: format!("mcp-http;token_id={id};subject=subject-{id};peer=unknown"),
                profile: Some("read".to_string()),
                client_session_id: None,
            }),
            last_seen: seen,
        };

        let first = binding("first", start);
        let first_handle = first.caller_handle.clone();
        bindings
            .bind_at("session-first".to_string(), first, start)
            .await
            .unwrap();
        bindings
            .bind_at(
                "session-second".to_string(),
                binding("second", start + Duration::from_secs(1)),
                start + Duration::from_secs(1),
            )
            .await
            .unwrap();
        bindings
            .bind_at(
                "session-third".to_string(),
                binding("third", start + Duration::from_secs(2)),
                start + Duration::from_secs(2),
            )
            .await
            .unwrap();

        assert!(bindings
            .get_at("session-first", start + Duration::from_secs(2))
            .await
            .is_none());
        assert!(caller_for_handle(&first_handle).is_none());
        assert_eq!(bindings.entries.read().await.len(), 2);
        assert!(bindings
            .get_at("session-second", start + Duration::from_secs(12))
            .await
            .is_none());
        assert!(bindings
            .get_at("session-third", start + Duration::from_secs(12))
            .await
            .is_none());
        assert!(bindings.entries.read().await.is_empty());
    }

    #[test]
    fn authenticated_http_context_maps_to_a_server_generated_audit_identity() {
        let context = RemoteMcpContext {
            token_id: Arc::from("token;one"),
            subject: Arc::from("build=agent"),
            profile: McpAccessProfile::Standard,
            session_id: Some(Arc::from("session-123")),
            peer_addr: Some("127.0.0.1:43127".parse().unwrap()),
        };
        let caller = context.to_control_caller();
        assert_eq!(caller.profile.as_deref(), Some("standard"));
        assert_eq!(caller.client_session_id.as_deref(), Some("session-123"));
        assert!(caller.source.contains("token_id=token\\;one"));
        assert!(caller.source.contains("subject=build\\=agent"));
        assert!(caller.source.contains("peer=127.0.0.1:43127"));
    }

    #[test]
    fn zero_session_binding_limits_are_rejected() {
        let mut config = McpHttpConfig::loopback(8765);
        config.session_binding_ttl = Duration::ZERO;
        assert_eq!(
            config.validate(),
            Err(ConfigError::InvalidSessionBindingTtl)
        );
        config.session_binding_ttl = Duration::from_secs(1);
        config.max_session_bindings = 0;
        assert_eq!(
            config.validate(),
            Err(ConfigError::InvalidMaxSessionBindings)
        );
    }

    #[tokio::test]
    async fn rmcp_session_is_bound_to_authenticated_principal_and_profile() {
        let mut config = McpHttpConfig::loopback(8765);
        config.max_profile = McpAccessProfile::Standard;
        let authorizer = Arc::new(
            StaticTokenAuthorizer::new(vec![
                token("owner", "srv_owner", McpAccessProfile::Standard),
                token("other", "srv_other", McpAccessProfile::Standard),
            ])
            .unwrap(),
        );
        let created_profiles = Arc::new(std::sync::Mutex::new(Vec::new()));
        let observed_profiles = Arc::clone(&created_profiles);
        let created_caller_handles = Arc::new(std::sync::Mutex::new(Vec::new()));
        let observed_caller_handles = Arc::clone(&created_caller_handles);
        let service = AgentMuxMcpHttpService::new(
            config,
            authorizer,
            move |profile| {
                observed_profiles.lock().unwrap().push(profile);
                observed_caller_handles
                    .lock()
                    .unwrap()
                    .push(current_caller_handle());
                Ok(TestHandler)
            },
            CancellationToken::new(),
        )
        .unwrap();

        let initialize = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": { "name": "sidecar-test", "version": "1" }
            }
        });
        let initialize_request = Request::builder()
            .method(Method::POST)
            .uri("/mcp")
            .header(HOST, "127.0.0.1:8765")
            .header(AUTHORIZATION, "Bearer srv_owner")
            .header(PROFILE_HEADER, "standard")
            .header("accept", "application/json, text/event-stream")
            .header("content-type", "application/json")
            .body(Body::from(initialize.to_string()))
            .unwrap();
        let initialize_response = service.handle(initialize_request).await;
        assert_eq!(initialize_response.status(), StatusCode::OK);
        let session_id = initialize_response
            .headers()
            .get(SESSION_HEADER)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert_eq!(
            created_profiles.lock().unwrap().as_slice(),
            &[McpAccessProfile::Standard]
        );
        let caller_handle = created_caller_handles.lock().unwrap()[0]
            .clone()
            .expect("authenticated caller handle must exist during service creation");
        let caller = caller_for_handle(&caller_handle).unwrap();
        assert_eq!(caller.profile.as_deref(), Some("standard"));
        assert_eq!(
            caller.client_session_id.as_deref(),
            Some(session_id.as_str())
        );
        assert!(caller.source.contains("token_id=owner"));
        assert!(caller.source.contains("subject=subject-owner"));

        let initialized_body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        let make_follow_up = |bearer: &str, profile: Option<&str>| {
            let mut builder = Request::builder()
                .method(Method::POST)
                .uri("/mcp")
                .header(HOST, "127.0.0.1:8765")
                .header(AUTHORIZATION, bearer)
                .header(SESSION_HEADER, &session_id)
                .header("mcp-protocol-version", "2025-03-26")
                .header("accept", "application/json, text/event-stream")
                .header("content-type", "application/json");
            if let Some(profile) = profile {
                builder = builder.header(PROFILE_HEADER, profile);
            }
            builder
                .body(Body::from(initialized_body.to_string()))
                .unwrap()
        };

        let intruder_response = service
            .handle(make_follow_up("Bearer srv_other", None))
            .await;
        assert_eq!(intruder_response.status(), StatusCode::FORBIDDEN);

        let changed_profile_response = service
            .handle(make_follow_up("Bearer srv_owner", Some("read")))
            .await;
        assert_eq!(changed_profile_response.status(), StatusCode::CONFLICT);

        let initialized_response = service
            .handle(make_follow_up("Bearer srv_owner", None))
            .await;
        assert_eq!(initialized_response.status(), StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn explicit_dynamic_bind_reports_endpoint_and_shuts_down() {
        let mut config = McpHttpConfig::loopback(0);
        config.endpoint_path = "/agentmux/mcp".to_string();
        let authorizer = Arc::new(
            StaticTokenAuthorizer::new(vec![token("server", "srv_server", McpAccessProfile::Read)])
                .unwrap(),
        );
        let server = spawn_mcp_http_server(config, authorizer, |_| Ok(TestHandler))
            .await
            .unwrap();
        assert_ne!(server.local_addr().port(), 0);
        assert!(server.endpoint_url().ends_with("/agentmux/mcp"));
        server.shutdown().await.unwrap();
    }

    #[test]
    fn cancellation_token_stops_new_requests_before_transport_dispatch() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn constant_time_comparison_handles_length_mismatches() {
        assert!(constant_time_eq(b"same", b"same"));
        assert!(!constant_time_eq(b"same", b"samf"));
        assert!(!constant_time_eq(b"same", b"same-longer"));
        assert!(!constant_time_eq(b"", b"non-empty"));
    }
}
