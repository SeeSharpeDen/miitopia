use base64::prelude::*;
use log::{debug, error, info, trace, warn};
use reqwest::{Client, Response, StatusCode};
use serde::Deserialize;
use serde_json::Value;
use serenity::prelude::{TypeMapKey, RwLock};
use std::{
    collections::HashMap,
    fmt::{self, Display}, sync::Arc,
};

#[derive(Clone)]
pub struct Spotify {
    token: String,
}

impl Spotify {
    pub async fn from_credentials(
        client_id: String,
        client_secret: String,
    ) -> Result<Spotify, SpotifyError> {
        // Get the token.
        let token = get_token(client_id, client_secret).await?;
        Ok(Spotify { token })
    }

    pub async fn get(self, url: String) -> Result<Response, SpotifyError> {
        let result = Client::new()
            .get(url)
            .header("Content-Type", "application/json")
            // .header("User-Agent", "Mozilla/5.0 (X11; Linux x86_64; rv:108.0) Gecko/20100101 Firefox/108.0")
            .bearer_auth(self.token)
            .send()
            .await?;
        Ok(result)
    }
}

async fn get_token(client_id: String, client_secret: String) -> Result<String, SpotifyError> {
    // Get a token. ref:
    // https://developer.spotify.com/documentation/general/guides/authorization/client-credentials/

    // Encode our client id and secrets in base64.
    let b64 = BASE64_STANDARD.encode(format!("{}:{}", client_id, client_secret));

    // Add the form element to tell spotify what token to use.
    let mut form_params = HashMap::new();
    form_params.insert("grant_type", "client_credentials");

    // Create the request.
    let request = reqwest::Client::new()
        .post("https://accounts.spotify.com/api/token")
        .form(&form_params)
        .header("Authorization", format!("Basic {}", b64));
    trace!("Requesting token from \"https://accounts.spotify.com/api/token\"",);

    // Send the request.
    let response = request.send().await?;

    match response.status() {
        // If we got a 200, parse the token.
        StatusCode::OK => {
            let json = response
                .json::<serde_json::Value>()
                .await
                .expect("Unable to parse spotify token json.");

            // Return an error if we cannot parse the token.
            match parse_token(json) {
                Some(token) => Ok(token),
                None => Err(SpotifyError::InvalidToken),
            }
        }
        // Not a 200 response?
        _ => Err(error_from(response).await),
    }
}

async fn error_from(response: Response) -> SpotifyError {
    // Save the status for later.
    let status = response.status();

    // Parse the json and get the "error" value.
    let json = match response.json::<Value>().await {
        Ok(json) => json,
        Err(why) => {
            warn!("Unable to parse json: {}", why);
            return SpotifyError::Generic(status);
        }
    };

    // If no error exists, make one up.
    let error = match json.get("error") {
        Some(error) => error,
        None => {
            return SpotifyError::ApiError(ApiError {
                message: "Unknown Error".to_owned(),
                status: status.as_u16(),
            })
        }
    };

    // If the error is a string, print it, else parse it as an ApiError.
    match error {
        Value::String(msg) => {
            return SpotifyError::ApiError(ApiError {
                message: msg.to_string(),
                status: status.as_u16(),
            });
        }
        _ => match serde_json::from_value::<ApiError>(error.to_owned()) {
            Ok(error) => return SpotifyError::ApiError(error),
            Err(_) => {
                return SpotifyError::ApiError(ApiError {
                    message: error.as_str().expect("No Error String").to_owned(),
                    status: status.as_u16(),
                })
            }
        },
    }
}

fn parse_token(json: serde_json::Value) -> Option<String> {
    // Make sure the token type is actually a bearer token.
    let t_type = json.get("token_type")?.as_str()?;
    if t_type != "Bearer" {
        warn!("Token Type is \"{}\" not \"Bearer\" token invalid.", t_type);
        return None;
    }
    let t_access = json.get("access_token")?.as_str()?;

    debug!("Parsed spotify token: {}", t_access);

    Some(t_access.to_owned())
}

impl TypeMapKey for Spotify {
    type Value = Arc<RwLock<Spotify>>;
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiError {
    pub message: String,
    pub status: u16,
}

#[derive(Debug)]
pub enum SpotifyError {
    Generic(StatusCode),
    ApiError(ApiError),
    Unauthorized,
    InvalidToken,
    NotFound,
    Reqwest(reqwest::Error),
}

impl fmt::Display for SpotifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpotifyError::Generic(e) => write!(f, "Http {}", e),
            SpotifyError::ApiError(e) => write!(f, "Api Error {} {}", e.status, e.message),
            SpotifyError::Unauthorized => write!(f, "Unauthorized"),
            SpotifyError::InvalidToken => write!(f, "Invalid Token"),
            SpotifyError::NotFound => write!(f, "Not Found or Not Available"),
            SpotifyError::Reqwest(e) => write!(f, "Reqwest: {}", e),
        }
    }
}

impl From<reqwest::Error> for SpotifyError {
    fn from(e: reqwest::Error) -> Self {
        SpotifyError::Reqwest(e)
    }
}
