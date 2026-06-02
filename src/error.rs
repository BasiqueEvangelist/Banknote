use axum::response::IntoResponse;
use reqwest::StatusCode;

pub enum ApiError {
    Reqwest(reqwest::Error),
    NotFound,
}

macro_rules! from_err {
    ($from_type:ty, $variant:ident) => {
        impl From<$from_type> for ApiError {
            fn from(value: $from_type) -> Self {
                ApiError::$variant(value)
            }
        }
    };
}

from_err!(reqwest::Error, Reqwest);

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        match self {
            ApiError::Reqwest(error) => problemdetails::new(StatusCode::BAD_GATEWAY)
                .with_type("https://basique.top/banknote/error/failed_request")
                .with_title("Failed to send request to mirrored server")
                .with_detail(error.to_string())
                .into_response(),
            ApiError::NotFound => problemdetails::new(StatusCode::NOT_FOUND)
                .with_type("about:blank")
                .with_title("Not Found")
                .into_response()
        }
    }
}
