//! 请求上下文模块
//!
//! 提供请求生命周期的上下文管理，封装通用初始化逻辑

use crate::app_config::AppType;
use crate::provider::Provider;
use crate::proxy::{
    extract_session_id,
    forwarder::RequestForwarder,
    server::ProxyState,
    session::SessionIdResult,
    session_model_registry::{SessionModelRegistry, SESSION_MODEL_REGISTRY},
    types::{AppProxyConfig, CopilotOptimizerConfig, OptimizerConfig, RectifierConfig},
    ProxyError,
};
use axum::http::HeaderMap;
use std::time::Instant;

pub(crate) const USAGE_SOURCE_HEADER: &str = "x-cc-switch-usage-source";
pub(crate) const FOLLOW_SESSION_HEADER: &str = "x-cc-switch-follow-session";

pub(crate) fn take_usage_source(headers: &mut HeaderMap) -> &'static str {
    let is_claude_mem = {
        let mut values = headers.get_all(USAGE_SOURCE_HEADER).iter();
        let first_matches =
            values.next().and_then(|value| value.to_str().ok()) == Some("claude-mem");
        first_matches && values.next().is_none()
    };
    headers.remove(USAGE_SOURCE_HEADER);
    if is_claude_mem {
        "MEM"
    } else {
        "proxy"
    }
}

pub(crate) fn take_follow_session(headers: &mut HeaderMap) -> Result<Option<String>, ProxyError> {
    let values = headers
        .get_all(FOLLOW_SESSION_HEADER)
        .iter()
        .map(|value| value.to_str().ok().map(str::to_owned))
        .collect::<Vec<_>>();
    headers.remove(FOLLOW_SESSION_HEADER);

    if values.is_empty() {
        return Ok(None);
    }
    if values.len() != 1 {
        return Err(ProxyError::FollowSessionInvalid);
    }
    let value = values
        .into_iter()
        .next()
        .flatten()
        .ok_or(ProxyError::FollowSessionInvalid)?;
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| matches!(byte, 0x21..=0x7e))
    {
        return Err(ProxyError::FollowSessionInvalid);
    }
    Ok(Some(value))
}

pub(crate) fn apply_session_model_follow(
    body: &mut serde_json::Value,
    headers: &mut HeaderMap,
    app_type_str: &str,
    session: &SessionIdResult,
    registry: &SessionModelRegistry,
) -> Result<&'static str, ProxyError> {
    let data_source = take_usage_source(headers);
    let follow_session = take_follow_session(headers)?;

    if data_source == "MEM" {
        if let Some(follow_session) = follow_session {
            if app_type_str != "claude" {
                return Err(ProxyError::FollowSessionInvalid);
            }
            let model = registry
                .resolve(&follow_session)
                .ok_or(ProxyError::SessionModelUnavailable)?;
            body["model"] = serde_json::Value::String(model);
        }
        return Ok(data_source);
    }

    if follow_session.is_some() {
        return Err(ProxyError::FollowSessionInvalid);
    }

    if app_type_str == "claude" && session.client_provided {
        if let Some(model) = body.get("model").and_then(|value| value.as_str()) {
            if !model.is_empty() && model != "unknown" {
                registry.record(&session.session_id, model);
            }
        }
    }
    Ok(data_source)
}

/// 流式超时配置
#[derive(Debug, Clone, Copy)]
pub struct StreamingTimeoutConfig {
    /// 首字节超时（秒），0 表示禁用
    pub first_byte_timeout: u64,
    /// 静默期超时（秒），0 表示禁用
    pub idle_timeout: u64,
}

/// 请求上下文
///
/// 贯穿整个请求生命周期，包含：
/// - 计时信息
/// - 应用级代理配置（per-app）
/// - 选中的 Provider 列表（用于故障转移）
/// - 请求模型名称
/// - 日志标签
/// - Session ID（用于日志关联）
pub struct RequestContext {
    /// 请求开始时间
    pub start_time: Instant,
    /// 应用级代理配置（per-app，包含重试次数和超时配置）
    pub app_config: AppProxyConfig,
    /// 选中的 Provider（故障转移链的第一个）
    pub provider: Provider,
    /// 完整的 Provider 列表（用于故障转移）
    providers: Vec<Provider>,
    /// 请求开始时的"当前供应商"（用于判断是否需要同步 UI/托盘）
    ///
    /// 这里使用本地 settings 的设备级 current provider。
    /// 代理模式下如果实际使用的 provider 与此不一致，会触发切换以确保 UI 始终准确。
    pub current_provider_id: String,
    /// 请求中的模型名称
    pub request_model: String,
    /// 实际发往上游的模型名（路由接管/模型映射后的真值，forward 成功后回填）。
    ///
    /// usage 归因的兜底顺序：上游响应回显 → outbound_model → request_model。
    /// 不能直接用 request_model 兜底：接管场景下它是映射前的客户端别名。
    pub outbound_model: Option<String>,
    /// 日志标签（如 "Claude"、"Codex"、"Gemini"）
    pub tag: &'static str,
    /// 应用类型字符串（如 "claude"、"codex"、"gemini"）
    pub app_type_str: &'static str,
    /// 应用类型（预留，目前通过 app_type_str 使用）
    #[allow(dead_code)]
    pub app_type: AppType,
    /// Session ID（从客户端请求提取或新生成）
    pub session_id: String,
    /// Session ID 是否由客户端提供。生成的 UUID 不能作为上游缓存 key，否则每个请求都会换 key。
    pub session_client_provided: bool,
    /// 本地使用量来源标签；仅允许 `MEM` 或默认 `proxy`。
    pub data_source: &'static str,
    /// 整流器配置
    pub rectifier_config: RectifierConfig,
    /// 优化器配置
    pub optimizer_config: OptimizerConfig,
    /// Copilot 优化器配置
    pub copilot_optimizer_config: CopilotOptimizerConfig,
}

impl RequestContext {
    /// 创建请求上下文
    ///
    /// # Arguments
    /// * `state` - 代理服务器状态
    /// * `body` - 请求体 JSON
    /// * `headers` - 请求头（用于提取 Session ID）
    /// * `app_type` - 应用类型
    /// * `tag` - 日志标签
    /// * `app_type_str` - 应用类型字符串
    ///
    /// # Errors
    /// 返回 `ProxyError` 如果 Provider 选择失败
    pub async fn new(
        state: &ProxyState,
        body: &mut serde_json::Value,
        headers: &mut HeaderMap,
        app_type: AppType,
        tag: &'static str,
        app_type_str: &'static str,
    ) -> Result<Self, ProxyError> {
        let start_time = Instant::now();

        // 从数据库读取应用级代理配置（per-app）
        let app_config = state
            .db
            .get_proxy_config_for_app(app_type_str)
            .await
            .map_err(|e| ProxyError::DatabaseError(e.to_string()))?;

        // 从数据库读取整流器配置
        let rectifier_config = state.db.get_rectifier_config().unwrap_or_default();
        let optimizer_config = state.db.get_optimizer_config().unwrap_or_default();
        let copilot_optimizer_config = state.db.get_copilot_optimizer_config().unwrap_or_default();

        let current_provider_id =
            crate::settings::get_current_provider(&app_type).unwrap_or_default();

        // 提取 Session ID
        let session_result = extract_session_id(headers, body, app_type_str);
        let session_id = session_result.session_id.clone();
        let data_source = apply_session_model_follow(
            body,
            headers,
            app_type_str,
            &session_result,
            &SESSION_MODEL_REGISTRY,
        )?;

        // 跟随逻辑完成后再记录模型，保证 MEM 使用映射前的原始 Claude 模型档位。
        let request_model = body
            .get("model")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown")
            .to_string();

        log::debug!(
            "[{}] Session ID: {} (from {:?}, client_provided: {})",
            tag,
            session_id,
            session_result.source,
            session_result.client_provided
        );

        // 使用共享的 ProviderRouter 选择 Provider（熔断器状态跨请求保持）
        // 注意：只在这里调用一次，结果传递给 forwarder，避免重复消耗 HalfOpen 名额
        let providers = state
            .provider_router
            .select_providers(app_type_str)
            .await
            .map_err(|e| match e {
                crate::error::AppError::AllProvidersCircuitOpen => {
                    ProxyError::AllProvidersCircuitOpen
                }
                crate::error::AppError::NoProvidersConfigured => ProxyError::NoProvidersConfigured,
                _ => ProxyError::DatabaseError(e.to_string()),
            })?;

        let provider = providers
            .first()
            .cloned()
            .ok_or(ProxyError::NoAvailableProvider)?;

        log::debug!(
            "[{}] Provider: {}, model: {}, failover chain: {} providers, session: {}",
            tag,
            provider.name,
            request_model,
            providers.len(),
            session_id
        );

        Ok(Self {
            start_time,
            app_config,
            provider,
            providers,
            current_provider_id,
            request_model,
            outbound_model: None,
            tag,
            app_type_str,
            app_type,
            session_id,
            session_client_provided: session_result.client_provided,
            data_source,
            rectifier_config,
            optimizer_config,
            copilot_optimizer_config,
        })
    }

    /// 从 URI 提取模型名称（Gemini 专用）
    ///
    /// Gemini API 的模型名称在 URI 中，格式如：
    /// `/v1beta/models/gemini-pro:generateContent`
    pub fn with_model_from_uri(mut self, uri: &axum::http::Uri) -> Self {
        // 用 path() 而不是 path_and_query()：模型名必须从路径段中解析，
        // 否则 GET /v1beta/models/<id>?key=... 会把 query 拼到 request_model 上。
        let endpoint = uri.path();

        self.request_model =
            extract_gemini_model_from_path(endpoint).unwrap_or_else(|| "unknown".to_string());

        self
    }

    /// 创建 RequestForwarder
    ///
    /// 使用共享的 ProviderRouter，确保熔断器状态跨请求保持
    ///
    /// 配置生效规则：
    /// - 故障转移开启：超时配置正常生效（0 表示禁用超时）
    /// - 故障转移关闭：超时配置不生效（全部传入 0）
    pub fn create_forwarder(&self, state: &ProxyState) -> RequestForwarder {
        let (non_streaming_timeout, first_byte_timeout, idle_timeout) =
            if self.app_config.auto_failover_enabled {
                // 故障转移开启：使用配置的值（0 = 禁用超时）
                (
                    self.app_config.non_streaming_timeout as u64,
                    self.app_config.streaming_first_byte_timeout as u64,
                    self.app_config.streaming_idle_timeout as u64,
                )
            } else {
                // 故障转移关闭：不启用超时配置
                log::debug!(
                    "[{}] Failover disabled, timeout configs are bypassed",
                    self.tag
                );
                (0, 0, 0)
            };

        // 故障转移关闭时强制 max_retries=0（仅尝试 1 个 provider），与「不超时 + 不切换」语义一致。
        let max_retries = if self.app_config.auto_failover_enabled {
            self.app_config.max_retries
        } else {
            0
        };

        RequestForwarder::new(
            state.provider_router.clone(),
            non_streaming_timeout,
            state.status.clone(),
            state.current_providers.clone(),
            state.gemini_shadow.clone(),
            state.codex_chat_history.clone(),
            state.failover_manager.clone(),
            state.app_handle.clone(),
            self.current_provider_id.clone(),
            self.session_id.clone(),
            self.session_client_provided,
            first_byte_timeout,
            idle_timeout,
            self.rectifier_config.clone(),
            self.optimizer_config.clone(),
            self.copilot_optimizer_config.clone(),
            max_retries,
        )
    }

    /// 获取 Provider 列表（用于故障转移）
    ///
    /// 返回在创建上下文时已选择的 providers，避免重复调用 select_providers()
    pub fn get_providers(&self) -> Vec<Provider> {
        self.providers.clone()
    }

    /// 计算请求延迟（毫秒）
    #[inline]
    pub fn latency_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    /// 获取流式超时配置
    ///
    /// 配置生效规则：
    /// - 故障转移开启：返回配置的值（0 表示禁用超时检查）
    /// - 故障转移关闭：返回 0（禁用超时检查）
    #[inline]
    pub fn streaming_timeout_config(&self) -> StreamingTimeoutConfig {
        if self.app_config.auto_failover_enabled {
            // 故障转移开启：使用配置的值（0 = 禁用超时）
            StreamingTimeoutConfig {
                first_byte_timeout: self.app_config.streaming_first_byte_timeout as u64,
                idle_timeout: self.app_config.streaming_idle_timeout as u64,
            }
        } else {
            // 故障转移关闭：禁用流式超时检查
            StreamingTimeoutConfig {
                first_byte_timeout: 0,
                idle_timeout: 0,
            }
        }
    }
}

/// Pull the Gemini model name out of an API path.
///
/// Accepts forms like `/v1beta/models/gemini-pro:generateContent`,
/// `/v1/models/gemini-1.5-flash`, `gemini/v1beta/models/<model>:streamGenerateContent`.
/// Returns `None` when no `models/<name>` segment is present.
pub(crate) fn extract_gemini_model_from_path(endpoint: &str) -> Option<String> {
    let segments: Vec<&str> = endpoint.split('/').collect();
    segments
        .iter()
        .position(|s| *s == "models")
        .and_then(|i| segments.get(i + 1).copied())
        // 防御性裁剪：即便调用方传入带 ? 或 :action 的字符串，也只保留 model id 本身
        .map(|s| s.split('?').next().unwrap_or(s))
        .map(|s| s.split(':').next().unwrap_or(s))
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        apply_session_model_follow, extract_gemini_model_from_path, take_follow_session,
        take_usage_source, FOLLOW_SESSION_HEADER, USAGE_SOURCE_HEADER,
    };
    use crate::proxy::{
        session_model_registry::SessionModelRegistry, ProxyError, SessionIdResult, SessionIdSource,
    };
    use axum::http::{HeaderMap, HeaderValue};
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn claude_mem_source_is_consumed() {
        let mut headers = HeaderMap::new();
        headers.insert(USAGE_SOURCE_HEADER, HeaderValue::from_static("claude-mem"));

        assert_eq!(take_usage_source(&mut headers), "MEM");
        assert!(!headers.contains_key(USAGE_SOURCE_HEADER));
    }

    #[test]
    fn missing_unknown_or_repeated_source_is_proxy_and_removed() {
        let mut missing = HeaderMap::new();
        assert_eq!(take_usage_source(&mut missing), "proxy");

        let mut unknown = HeaderMap::new();
        unknown.insert(USAGE_SOURCE_HEADER, HeaderValue::from_static("other"));
        assert_eq!(take_usage_source(&mut unknown), "proxy");
        assert!(!unknown.contains_key(USAGE_SOURCE_HEADER));

        let mut repeated = HeaderMap::new();
        repeated.append(USAGE_SOURCE_HEADER, HeaderValue::from_static("claude-mem"));
        repeated.append(USAGE_SOURCE_HEADER, HeaderValue::from_static("claude-mem"));
        assert_eq!(take_usage_source(&mut repeated), "proxy");
        assert!(!repeated.contains_key(USAGE_SOURCE_HEADER));
    }

    #[test]
    fn follow_session_header_is_consumed_and_validated() {
        let mut headers = HeaderMap::new();
        headers.insert(
            FOLLOW_SESSION_HEADER,
            HeaderValue::from_static("session-123"),
        );
        assert_eq!(
            take_follow_session(&mut headers).unwrap().as_deref(),
            Some("session-123")
        );
        assert!(!headers.contains_key(FOLLOW_SESSION_HEADER));

        let mut repeated = HeaderMap::new();
        repeated.append(
            FOLLOW_SESSION_HEADER,
            HeaderValue::from_static("session-123"),
        );
        repeated.append(
            FOLLOW_SESSION_HEADER,
            HeaderValue::from_static("session-123"),
        );
        assert!(matches!(
            take_follow_session(&mut repeated),
            Err(ProxyError::FollowSessionInvalid)
        ));
        assert!(!repeated.contains_key(FOLLOW_SESSION_HEADER));
    }

    #[test]
    fn normal_request_registers_and_mem_request_follows_without_changing_normal_body() {
        let registry = SessionModelRegistry::new(1024, Duration::from_secs(7200));
        let session = SessionIdResult {
            session_id: "session-123".to_string(),
            source: SessionIdSource::MetadataSessionId,
            client_provided: true,
        };
        let mut normal_headers = HeaderMap::new();
        let mut normal_body = json!({"model": "claude-haiku-4-5", "messages": []});

        let source = apply_session_model_follow(
            &mut normal_body,
            &mut normal_headers,
            "claude",
            &session,
            &registry,
        )
        .unwrap();
        assert_eq!(source, "proxy");
        assert_eq!(normal_body["model"], "claude-haiku-4-5");

        let mut mem_headers = HeaderMap::new();
        mem_headers.insert(USAGE_SOURCE_HEADER, HeaderValue::from_static("claude-mem"));
        mem_headers.insert(
            FOLLOW_SESSION_HEADER,
            HeaderValue::from_static("session-123"),
        );
        let mut mem_body = json!({"model": "claude-haiku-4-5", "messages": []});
        let source = apply_session_model_follow(
            &mut mem_body,
            &mut mem_headers,
            "claude",
            &session,
            &registry,
        )
        .unwrap();

        assert_eq!(source, "MEM");
        assert_eq!(mem_body["model"], "claude-haiku-4-5");
        assert!(!mem_headers.contains_key(USAGE_SOURCE_HEADER));
        assert!(!mem_headers.contains_key(FOLLOW_SESSION_HEADER));
    }

    #[test]
    fn model_switch_updates_next_mem_request_and_sessions_do_not_cross() {
        let registry = SessionModelRegistry::new(1024, Duration::from_secs(7200));
        for (session_id, model) in [
            ("session-a", "claude-haiku-4-5"),
            ("session-b", "claude-opus-4-8"),
            ("session-a", "claude-sonnet-4-6"),
        ] {
            let session = SessionIdResult {
                session_id: session_id.to_string(),
                source: SessionIdSource::MetadataSessionId,
                client_provided: true,
            };
            let mut body = json!({"model": model});
            apply_session_model_follow(
                &mut body,
                &mut HeaderMap::new(),
                "claude",
                &session,
                &registry,
            )
            .unwrap();
        }

        for (session_id, expected) in [
            ("session-a", "claude-sonnet-4-6"),
            ("session-b", "claude-opus-4-8"),
        ] {
            let session = SessionIdResult {
                session_id: "generated-for-mem".to_string(),
                source: SessionIdSource::Generated,
                client_provided: false,
            };
            let mut headers = HeaderMap::new();
            headers.insert(USAGE_SOURCE_HEADER, HeaderValue::from_static("claude-mem"));
            headers.insert(
                FOLLOW_SESSION_HEADER,
                HeaderValue::from_str(session_id).unwrap(),
            );
            let mut body = json!({"model": "placeholder"});
            apply_session_model_follow(&mut body, &mut headers, "claude", &session, &registry)
                .unwrap();
            assert_eq!(body["model"], expected);
        }
    }

    #[test]
    fn missing_follow_state_fails_closed_and_generated_sessions_are_not_registered() {
        let registry = SessionModelRegistry::new(1024, Duration::from_secs(7200));
        let generated = SessionIdResult {
            session_id: "generated".to_string(),
            source: SessionIdSource::Generated,
            client_provided: false,
        };
        let mut normal_body = json!({"model": "claude-opus-4-8"});
        apply_session_model_follow(
            &mut normal_body,
            &mut HeaderMap::new(),
            "claude",
            &generated,
            &registry,
        )
        .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(USAGE_SOURCE_HEADER, HeaderValue::from_static("claude-mem"));
        headers.insert(FOLLOW_SESSION_HEADER, HeaderValue::from_static("generated"));
        let mut mem_body = json!({"model": "placeholder"});
        assert!(matches!(
            apply_session_model_follow(
                &mut mem_body,
                &mut headers,
                "claude",
                &generated,
                &registry,
            ),
            Err(ProxyError::SessionModelUnavailable)
        ));
        assert_eq!(mem_body["model"], "placeholder");
    }

    #[test]
    fn extract_model_with_action() {
        assert_eq!(
            extract_gemini_model_from_path("/v1beta/models/gemini-pro:generateContent").as_deref(),
            Some("gemini-pro"),
        );
    }

    #[test]
    fn extract_model_with_dotted_version() {
        assert_eq!(
            extract_gemini_model_from_path("/v1beta/models/gemini-1.5-flash:streamGenerateContent")
                .as_deref(),
            Some("gemini-1.5-flash"),
        );
    }

    #[test]
    fn extract_model_without_action() {
        assert_eq!(
            extract_gemini_model_from_path("/v1/models/gemini-1.5-pro").as_deref(),
            Some("gemini-1.5-pro"),
        );
    }

    #[test]
    fn extract_model_with_proxy_prefix() {
        assert_eq!(
            extract_gemini_model_from_path("/gemini/v1beta/models/gemini-2.0-flash:countTokens")
                .as_deref(),
            Some("gemini-2.0-flash"),
        );
    }

    #[test]
    fn extract_model_with_query_string() {
        assert_eq!(
            extract_gemini_model_from_path("/v1beta/models/gemini-pro:generateContent?key=abc")
                .as_deref(),
            Some("gemini-pro"),
        );
    }

    #[test]
    fn extract_model_missing_segment() {
        assert_eq!(extract_gemini_model_from_path("/v1beta/operations"), None);
    }

    #[test]
    fn extract_model_trailing_models_segment() {
        // `/v1beta/models` (list endpoint) has no following segment → None.
        assert_eq!(extract_gemini_model_from_path("/v1beta/models"), None);
    }

    #[test]
    fn extract_model_get_with_query_only() {
        // GET /v1beta/models/<id>?key=... 无 action verb，仅靠 ':' 拆分会把 query 带进 model 名。
        // 修复后应该把 query 剥掉。
        assert_eq!(
            extract_gemini_model_from_path("/v1beta/models/gemini-pro?key=abc").as_deref(),
            Some("gemini-pro"),
        );
    }

    #[test]
    fn extract_model_get_with_proxy_prefix_and_query() {
        assert_eq!(
            extract_gemini_model_from_path("/gemini/v1beta/models/gemini-2.0-flash?key=abc")
                .as_deref(),
            Some("gemini-2.0-flash"),
        );
    }
}
