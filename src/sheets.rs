use google_sheets4::{hyper_util, hyper_rustls, Sheets, yup_oauth2};
use google_sheets4::api::ValueRange;
use std::error::Error;
use serde_json::{Value};
use yup_oauth2::{read_service_account_key, ServiceAccountAuthenticator};

pub type SheetsClient = hyper_rustls::HttpsConnector<
    hyper_util::client::legacy::connect::HttpConnector
>;

pub async fn create_sheets_client(key_path: &str) -> Result<
    Sheets<SheetsClient>,
    Box<dyn Error>> {
    let service_account_key = read_service_account_key(key_path).await?;
    let auth = ServiceAccountAuthenticator::builder(service_account_key)
        .build().await?;
    
    let client = hyper_util::client::legacy::Client::builder(
        hyper_util::rt::TokioExecutor::new()
    ).build(
        hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .unwrap().https_or_http()
            .enable_http1()
            .build()
    );
    
    Ok(Sheets::new(client, auth))
}

pub async fn append_row(
    sheets: &Sheets<SheetsClient>,
    spreadsheet_id: &str,
    range: &str,
    rows: Vec<Vec<Value>>
) -> Result<(), Box<dyn Error>> {
    let values = ValueRange {
        range: Some(range.to_string()),
        major_dimension: Some("ROWS".to_string()),
        values: Some(rows)
    };
    
    sheets
        .spreadsheets()
        .values_append(values, spreadsheet_id, range)
        .value_input_option("RAW")
        .insert_data_option("INSERT_ROWS")
        .doit()
        .await?;
    
    Ok(())
}

