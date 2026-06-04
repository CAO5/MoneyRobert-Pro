use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Debug)]
pub struct OkxClient {
    api_key: String,
    secret_key: String,
    passphrase: String,
    base_url: String,
    is_demo: bool,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct OkxAccount {
    #[serde(default)]
    pub utd_type: Option<String>,
    #[serde(default)]
    pub adj_eq: Option<String>,
    #[serde(default)]
    pub imr: Option<String>,
    #[serde(default)]
    pub iso_eq: Option<String>,
    #[serde(default)]
    pub mgn_ratio: Option<String>,
    #[serde(default)]
    pub mmr: Option<String>,
    #[serde(default)]
    pub notional_usd: Option<String>,
    #[serde(default)]
    pub ord_froz: Option<String>,
    #[serde(default)]
    pub total_eq: Option<String>,
    #[serde(default)]
    pub u_time: Option<String>,
    #[serde(default)]
    pub total_bal: Option<String>,
    #[serde(default)]
    pub iso_bal: Option<String>,
    #[serde(default)]
    pub eq: Option<String>,
    #[serde(default)]
    pub cash_bal: Option<String>,
    #[serde(default)]
    pub upl: Option<String>,
    #[serde(default)]
    pub upl_liab: Option<String>,
    #[serde(default)]
    pub cross_liab: Option<String>,
    #[serde(default)]
    pub iso_liab: Option<String>,
    #[serde(default)]
    pub bal: Option<String>,
    #[serde(default)]
    pub avail_bal: Option<String>,
    #[serde(default)]
    pub frozen_bal: Option<String>,
    #[serde(default)]
    pub spot_in_use_amt: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct OkxPosition {
    #[serde(default)]
    pub inst_type: Option<String>,
    #[serde(default)]
    pub mgn_mode: Option<String>,
    #[serde(default)]
    pub pos: Option<String>,
    #[serde(default)]
    pub pos_ccy: Option<String>,
    #[serde(default)]
    pub avail_pos: Option<String>,
    #[serde(default)]
    pub avg_px: Option<String>,
    #[serde(default)]
    pub upl: Option<String>,
    #[serde(default)]
    pub upl_ratio: Option<String>,
    #[serde(default)]
    pub inst_id: Option<String>,
    #[serde(default)]
    pub lever: Option<String>,
    #[serde(default)]
    pub liq_px: Option<String>,
    #[serde(default)]
    pub mark_px: Option<String>,
    #[serde(default)]
    pub imr: Option<String>,
    #[serde(default)]
    pub margin: Option<String>,
    #[serde(default)]
    pub mmr: Option<String>,
    #[serde(default)]
    pub notional_usd: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct OkxTicker {
    #[serde(default)]
    pub inst_type: Option<String>,
    #[serde(default)]
    pub inst_id: Option<String>,
    #[serde(default)]
    pub last: Option<String>,
    #[serde(default)]
    pub last_sz: Option<String>,
    #[serde(default)]
    pub ask_px: Option<String>,
    #[serde(default)]
    pub ask_sz: Option<String>,
    #[serde(default)]
    pub bid_px: Option<String>,
    #[serde(default)]
    pub bid_sz: Option<String>,
    #[serde(default)]
    pub open_24h: Option<String>,
    #[serde(default)]
    pub high_24h: Option<String>,
    #[serde(default)]
    pub low_24h: Option<String>,
    #[serde(default)]
    pub vol_ccy_24h: Option<String>,
    #[serde(default)]
    pub vol_24h: Option<String>,
    #[serde(default)]
    pub ts: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct OkxCandle {
    #[serde(default)]
    pub ts: Option<String>,
    #[serde(default)]
    pub o: Option<String>,
    #[serde(default)]
    pub h: Option<String>,
    #[serde(default)]
    pub l: Option<String>,
    #[serde(default)]
    pub c: Option<String>,
    #[serde(default)]
    pub vol: Option<String>,
    #[serde(default)]
    pub vol_ccy: Option<String>,
    #[serde(default)]
    pub vol_ccy_quote: Option<String>,
    #[serde(default)]
    pub confirm: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OkxOrderRequest {
    pub inst_id: String,
    pub td_mode: String,
    pub side: String,
    pub ord_type: String,
    pub sz: String,
    pub px: Option<String>,
    pub sl_trigger_px: Option<String>,
    pub sl_ord_px: Option<String>,
    pub tp_trigger_px: Option<String>,
    pub tp_ord_px: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OkxOrderResponse {
    pub ord_id: String,
    pub cl_ord_id: String,
    pub s_code: String,
    pub s_msg: String,
    pub tag: String,
}

impl OkxClient {
    pub fn new(api_key: String, secret_key: String, passphrase: String, is_demo: bool) -> Self {
        let base_url = "https://www.okx.com".to_string();

        let mut builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30));

        // Configure proxy from environment variables
        if let Ok(proxy_url) = std::env::var("ALL_PROXY")
            .or_else(|_| std::env::var("HTTPS_PROXY"))
            .or_else(|_| std::env::var("HTTP_PROXY"))
        {
            // Use socks5 (local DNS) instead of socks5h (remote DNS)
            // because host.docker.internal can only be resolved inside Docker
            let proxy_url = proxy_url.replace("socks5h://", "socks5://");
            if let Ok(proxy) = reqwest::Proxy::all(&proxy_url) {
                tracing::info!("OKX client using proxy: {}", proxy_url);
                builder = builder.proxy(proxy);
            }
        }

        let client = builder
            .build()
            .expect("Failed to create HTTP client");

        Self {
            api_key,
            secret_key,
            passphrase,
            base_url,
            is_demo,
            client,
        }
    }

    fn sign(&self, timestamp: &str, method: &str, request_path: &str, body: &str) -> String {
        let prehash = format!("{}{}{}{}", timestamp, method, request_path, body);
        let mut mac = HmacSha256::new_from_slice(self.secret_key.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(prehash.as_bytes());
        let result = mac.finalize();
        STANDARD.encode(result.into_bytes())
    }

    fn generate_timestamp() -> String {
        // OKX requires ISO 8601 format with milliseconds, e.g. "2020-12-08T09:08:57.715Z"
        // Use to_rfc3339_opts to guarantee millisecond precision
        Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }

    async fn get<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        params: Option<&[(&str, &str)]>,
    ) -> Result<T> {
        let timestamp = Self::generate_timestamp();
        
        let query_string = match params {
            Some(p) => {
                let mut pairs: Vec<String> = p.iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect();
                pairs.sort();
                format!("?{}", pairs.join("&"))
            }
            None => String::new(),
        };

        let request_path = format!("{}{}", path, query_string);
        let sign = self.sign(&timestamp, "GET", &request_path, "");

        let url = format!("{}{}", self.base_url, request_path);

        tracing::debug!("OKX GET request: timestamp={}, path={}, sign_prefix={}", 
            timestamp, request_path, format!("{}GET{}", timestamp, request_path));

        let mut req = self.client
            .get(&url)
            .header("OK-ACCESS-KEY", &self.api_key)
            .header("OK-ACCESS-SIGN", &sign)
            .header("OK-ACCESS-TIMESTAMP", &timestamp)
            .header("OK-ACCESS-PASSPHRASE", &self.passphrase);

        if self.is_demo {
            req = req.header("x-simulated-trading", "1");
        }

        let response = req
            .send()
            .await
            .map_err(|e| AppError::ExternalApi {
                service: "OKX".to_string(),
                message: e.to_string(),
            })?;

        let status = response.status();
        let body = response.text().await.map_err(|e| AppError::ExternalApi {
            service: "OKX".to_string(),
            message: e.to_string(),
        })?;

        if !status.is_success() {
            return Err(AppError::ExternalApi {
                service: "OKX".to_string(),
                message: format!("HTTP {}: {}", status, body),
            });
        }

        let result: T = serde_json::from_str(&body).map_err(|e| AppError::Serialization(e))?;

        Ok(result)
    }

    /// GET request returning raw serde_json::Value
    pub async fn get_raw(
        &self,
        path: &str,
        params: Option<&[(&str, String)]>,
    ) -> Result<serde_json::Value> {
        let timestamp = Self::generate_timestamp();

        let query_string = match params {
            Some(p) => {
                let mut pairs: Vec<String> = p.iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect();
                pairs.sort();
                format!("?{}", pairs.join("&"))
            }
            None => String::new(),
        };

        let request_path = format!("{}{}", path, query_string);
        let sign = self.sign(&timestamp, "GET", &request_path, "");

        let url = format!("{}{}", self.base_url, request_path);

        let mut req = self.client
            .get(&url)
            .header("OK-ACCESS-KEY", &self.api_key)
            .header("OK-ACCESS-SIGN", &sign)
            .header("OK-ACCESS-TIMESTAMP", &timestamp)
            .header("OK-ACCESS-PASSPHRASE", &self.passphrase);

        if self.is_demo {
            req = req.header("x-simulated-trading", "1");
        }

        let response = req
            .send()
            .await
            .map_err(|e| AppError::ExternalApi {
                service: "OKX".to_string(),
                message: e.to_string(),
            })?;

        let status = response.status();
        let body = response.text().await.map_err(|e| AppError::ExternalApi {
            service: "OKX".to_string(),
            message: e.to_string(),
        })?;

        if !status.is_success() {
            return Err(AppError::ExternalApi {
                service: "OKX".to_string(),
                message: format!("HTTP {}: {}", status, body),
            });
        }

        let result: serde_json::Value = serde_json::from_str(&body).map_err(|e| AppError::Serialization(e))?;

        Ok(result)
    }

    async fn post<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &impl serde::Serialize,
    ) -> Result<T> {
        let timestamp = Self::generate_timestamp();
        let body_str = serde_json::to_string(body).map_err(|e| AppError::Serialization(e))?;

        let sign = self.sign(&timestamp, "POST", path, &body_str);

        let url = format!("{}{}", self.base_url, path);

        let mut req = self.client
            .post(&url)
            .header("OK-ACCESS-KEY", &self.api_key)
            .header("OK-ACCESS-SIGN", &sign)
            .header("OK-ACCESS-TIMESTAMP", &timestamp)
            .header("OK-ACCESS-PASSPHRASE", &self.passphrase)
            .header("Content-Type", "application/json")
            .body(body_str);

        if self.is_demo {
            req = req.header("x-simulated-trading", "1");
        }

        let response = req
            .send()
            .await
            .map_err(|e| AppError::ExternalApi {
                service: "OKX".to_string(),
                message: e.to_string(),
            })?;

        let status = response.status();
        let resp_body = response.text().await.map_err(|e| AppError::ExternalApi {
            service: "OKX".to_string(),
            message: e.to_string(),
        })?;

        if !status.is_success() {
            return Err(AppError::ExternalApi {
                service: "OKX".to_string(),
                message: format!("HTTP {}: {}", status, resp_body),
            });
        }

        let result: T = serde_json::from_str(&resp_body).map_err(|e| AppError::Serialization(e))?;

        Ok(result)
    }

    pub async fn get_account_balance(&self) -> Result<Vec<OkxAccount>> {
        #[derive(Deserialize)]
        struct OkxResponse {
            code: String,
            msg: String,
            data: Vec<OkxAccount>,
        }

        let resp: OkxResponse = self.get("/api/v5/account/balance", None).await?;

        if resp.code != "0" {
            return Err(AppError::ExternalApi {
                service: "OKX".to_string(),
                message: resp.msg,
            });
        }

        Ok(resp.data)
    }

    pub async fn get_positions(&self, inst_type: Option<&str>) -> Result<Vec<OkxPosition>> {
        #[derive(Deserialize)]
        struct OkxResponse {
            code: String,
            msg: String,
            data: Vec<OkxPosition>,
        }

        let params = inst_type.map(|t| vec![("instType", t)]);
        let resp: OkxResponse = self.get("/api/v5/account/positions", params.as_deref()).await?;

        if resp.code != "0" {
            return Err(AppError::ExternalApi {
                service: "OKX".to_string(),
                message: resp.msg,
            });
        }

        Ok(resp.data)
    }

    pub async fn get_ticker(&self, inst_id: &str) -> Result<OkxTicker> {
        #[derive(Deserialize)]
        struct OkxResponse {
            code: String,
            msg: String,
            data: Vec<OkxTicker>,
        }

        let params = vec![("instId", inst_id)];
        let resp: OkxResponse = self.get("/api/v5/market/ticker", Some(&params)).await?;

        if resp.code != "0" {
            return Err(AppError::ExternalApi {
                service: "OKX".to_string(),
                message: resp.msg,
            });
        }

        resp.data.into_iter().next().ok_or_else(|| {
            AppError::NotFound(format!("Ticker not found for {}", inst_id))
        })
    }

    pub async fn get_candles(
        &self,
        inst_id: &str,
        bar: &str,
        limit: Option<usize>,
    ) -> Result<Vec<OkxCandle>> {
        #[derive(Deserialize)]
        struct OkxResponse {
            code: String,
            msg: String,
            data: Vec<OkxCandle>,
        }

        let mut params = vec![
            ("instId", inst_id),
            ("bar", bar),
        ];
        
        let limit_str;
        if let Some(l) = limit {
            limit_str = l.to_string();
            params.push(("limit", &limit_str));
        }

        let resp: OkxResponse = self.get("/api/v5/market/candles", Some(&params)).await?;

        if resp.code != "0" {
            return Err(AppError::ExternalApi {
                service: "OKX".to_string(),
                message: resp.msg,
            });
        }

        Ok(resp.data)
    }

    pub async fn place_order(&self, request: &OkxOrderRequest) -> Result<OkxOrderResponse> {
        #[derive(Deserialize)]
        struct OkxResponse {
            code: String,
            msg: String,
            data: Vec<OkxOrderResponse>,
        }

        let resp: OkxResponse = self.post("/api/v5/trade/order", request).await?;

        if resp.code != "0" {
            return Err(AppError::ExternalApi {
                service: "OKX".to_string(),
                message: resp.msg,
            });
        }

        resp.data.into_iter().next().ok_or_else(|| {
            AppError::Internal("No order response data".to_string())
        })
    }

    pub async fn cancel_order(
        &self,
        inst_id: &str,
        ord_id: &str,
    ) -> Result<OkxOrderResponse> {
        #[derive(Serialize)]
        struct CancelRequest {
            inst_id: String,
            ord_id: String,
        }

        #[derive(Deserialize)]
        struct OkxResponse {
            code: String,
            msg: String,
            data: Vec<OkxOrderResponse>,
        }

        let request = CancelRequest {
            inst_id: inst_id.to_string(),
            ord_id: ord_id.to_string(),
        };

        let resp: OkxResponse = self.post("/api/v5/trade/cancel-order", &request).await?;

        if resp.code != "0" {
            return Err(AppError::ExternalApi {
                service: "OKX".to_string(),
                message: resp.msg,
            });
        }

        resp.data.into_iter().next().ok_or_else(|| {
            AppError::Internal("No cancel response data".to_string())
        })
    }

    pub async fn set_leverage(
        &self,
        inst_id: &str,
        lever: &str,
        mgn_mode: &str,
    ) -> Result<()> {
        #[derive(Serialize)]
        struct LeverageRequest {
            inst_id: String,
            lever: String,
            mgn_mode: String,
        }

        #[derive(Deserialize)]
        struct OkxResponse {
            code: String,
            msg: String,
        }

        let request = LeverageRequest {
            inst_id: inst_id.to_string(),
            lever: lever.to_string(),
            mgn_mode: mgn_mode.to_string(),
        };

        let resp: OkxResponse = self.post("/api/v5/account/set-leverage", &request).await?;

        if resp.code != "0" {
            return Err(AppError::ExternalApi {
                service: "OKX".to_string(),
                message: resp.msg,
            });
        }

        Ok(())
    }
}
