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

#[derive(Debug, Serialize, Deserialize)]
pub struct OkxAccount {
    pub utd_type: String,
    pub adj_eq: String,
    pub imr: String,
    pub iso_eq: String,
    pub mgn_ratio: String,
    pub mmr: String,
    pub notional_usd: String,
    pub ord_froz: String,
    pub total_eq: String,
    pub u_time: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OkxPosition {
    pub inst_type: String,
    pub mgn_mode: String,
    pub pos: String,
    pub pos_ccy: String,
    pub avail_pos: String,
    pub avg_px: String,
    pub upl: String,
    pub upl_ratio: String,
    pub inst_id: String,
    pub lever: String,
    pub liq_px: String,
    pub mark_px: String,
    pub imr: String,
    pub margin: String,
    pub mmr: String,
    pub notional_usd: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OkxTicker {
    pub inst_type: String,
    pub inst_id: String,
    pub last: String,
    pub last_sz: String,
    pub ask_px: String,
    pub ask_sz: String,
    pub bid_px: String,
    pub bid_sz: String,
    pub open_24h: String,
    pub high_24h: String,
    pub low_24h: String,
    pub vol_ccy_24h: String,
    pub vol_24h: String,
    pub ts: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OkxCandle {
    pub ts: String,
    pub o: String,
    pub h: String,
    pub l: String,
    pub c: String,
    pub vol: String,
    pub vol_ccy: String,
    pub vol_ccy_quote: String,
    pub confirm: String,
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

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
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
        Utc::now().format("%Y-%m-%dT%H:%M:%S.%.3fZ").to_string()
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
