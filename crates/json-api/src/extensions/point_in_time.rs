//! Point-in-time query parsing helpers.

use jiff::Timestamp;
use salvo::{oapi::extract::QueryParam, prelude::StatusError};

use crate::extensions::*;

pub(crate) trait PointInTimeExt {
    fn into_point_in_time(self) -> Result<Timestamp, StatusError>;
}

impl PointInTimeExt for QueryParam<String, false> {
    fn into_point_in_time(self) -> Result<Timestamp, StatusError> {
        self.into_inner()
            .map(|value| value.parse::<Timestamp>())
            .transpose()
            .or_400("could not parse \"at\" query parameter")
            .map(|point_in_time| point_in_time.unwrap_or_else(Timestamp::now))
    }
}
