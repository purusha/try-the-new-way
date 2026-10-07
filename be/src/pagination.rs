//! Paginazione `page`/`page_size` e risposta `{ items, page, page_size, total }`.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;

pub const DEFAULT_PAGE_SIZE: i64 = 50;
pub const MAX_PAGE_SIZE: i64 = 200;

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct PageParams {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

impl PageParams {
    pub fn resolve(&self) -> Result<(i64, i64), ApiError> {
        let page = self.page.unwrap_or(1);
        let size = self.page_size.unwrap_or(DEFAULT_PAGE_SIZE);
        if page < 1 {
            return Err(ApiError::validation("page deve essere >= 1"));
        }
        if !(1..=MAX_PAGE_SIZE).contains(&size) {
            return Err(ApiError::validation(format!(
                "page_size deve essere tra 1 e {MAX_PAGE_SIZE}"
            )));
        }
        Ok((page, size))
    }

    /// `(limit, offset, page, page_size)` per le query SQL.
    pub fn sql(&self) -> Result<(i64, i64, i64, i64), ApiError> {
        let (page, size) = self.resolve()?;
        Ok((size, (page - 1) * size, page, size))
    }
}

#[derive(Debug, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub page: i64,
    pub page_size: i64,
    pub total: i64,
}

#[derive(Debug, Serialize)]
pub struct Items<T> {
    pub items: Vec<T>,
}
