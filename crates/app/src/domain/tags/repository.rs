//! Tags Repository

use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use sqlx::{Postgres, Transaction, query, query_as};
use tracing::debug;
use uuid::Uuid;

use super::{Taggable, records::TagUuid};

const SYNC_TAGS_SQL: &str = include_str!("sql/sync_tags.sql");
const CREATE_TAGGABLES_SQL: &str = include_str!("sql/create_taggables.sql");
const DELETE_TAGGABLES_SQL: &str = include_str!("sql/delete_taggables.sql");

#[cfg(test)]
const LIST_TAGGABLE_TAG_NAMES_SQL: &str = include_str!("sql/list_taggable_tag_names.sql");

#[derive(Debug, Clone, Default)]
pub(crate) struct PgTagsRepository;

impl PgTagsRepository {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self
    }

    #[tracing::instrument(
        name = "tags.repository.sync_tags",
        skip(self, tx),
        fields(
            names_count = tracing::field::Empty,
            synced_tag_count = tracing::field::Empty
        ),
        err
    )]
    pub(crate) async fn sync_tags(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        names: &[&str],
    ) -> Result<FxHashMap<String, TagUuid>, sqlx::Error> {
        tracing::Span::current().record("names_count", names.len());

        if names.is_empty() {
            return Ok(FxHashMap::default());
        }

        let new_uuids: Vec<Uuid> = names.iter().map(|_| Uuid::now_v7()).collect();
        let names_vec: Vec<String> = names.iter().map(|s| (*s).to_owned()).collect();

        let rows: Vec<(Uuid, String)> = query_as(SYNC_TAGS_SQL)
            .bind(&new_uuids)
            .bind(&names_vec)
            .fetch_all(&mut **tx)
            .await?;

        let tag_map: FxHashMap<String, TagUuid> = rows
            .into_iter()
            .map(|(uuid, name)| (name, TagUuid::from_uuid(uuid)))
            .collect();

        let synced_tag_count = tag_map.len();

        tracing::Span::current().record("synced_tag_count", synced_tag_count);

        debug!(synced_tag_count, "synchronized tags");

        Ok(tag_map)
    }

    #[tracing::instrument(
        name = "tags.repository.create_taggables",
        skip(self, tx, pairs),
        fields(
            taggable_type = tracing::field::Empty,
            pair_count = tracing::field::Empty
        ),
        err
    )]
    pub(crate) async fn create_taggables<T>(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        pairs: &[(TagUuid, T)],
    ) -> Result<(), sqlx::Error>
    where
        T: Taggable + Copy + Into<Uuid>,
    {
        let pair_count = pairs.len();

        tracing::Span::current().record("taggable_type", T::type_as_str());
        tracing::Span::current().record("pair_count", pair_count);

        if pairs.is_empty() {
            return Ok(());
        }

        let tag_uuids: Vec<Uuid> = pairs
            .iter()
            .map(|(tag_uuid, _)| tag_uuid.into_uuid())
            .collect();

        let taggable_uuids: Vec<Uuid> = pairs
            .iter()
            .map(|(_, taggable_uuid)| *taggable_uuid)
            .map(Into::into)
            .collect();

        sqlx::query(CREATE_TAGGABLES_SQL)
            .bind(&tag_uuids)
            .bind(&taggable_uuids)
            .bind(T::type_as_str())
            .execute(&mut **tx)
            .await?;

        debug!(
            pair_count,
            taggable_type = T::type_as_str(),
            "created taggables"
        );

        Ok(())
    }

    #[tracing::instrument(
        name = "tags.repository.delete_taggables",
        skip(self, tx, taggable_uuids),
        fields(
            taggable_type = tracing::field::Empty,
            taggable_count = tracing::field::Empty
        ),
        err
    )]
    pub(crate) async fn delete_taggables<T>(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        taggable_uuids: &[T],
    ) -> Result<(), sqlx::Error>
    where
        T: Taggable + Copy + Into<Uuid>,
    {
        let taggable_count = taggable_uuids.len();

        tracing::Span::current().record("taggable_type", T::type_as_str());
        tracing::Span::current().record("taggable_count", taggable_count);

        if taggable_uuids.is_empty() {
            return Ok(());
        }

        let uuids: Vec<Uuid> = taggable_uuids.iter().copied().map(Into::into).collect();

        query(DELETE_TAGGABLES_SQL)
            .bind(T::type_as_str())
            .bind(&uuids)
            .execute(&mut **tx)
            .await?;

        debug!(
            taggable_type = T::type_as_str(),
            taggable_count, "deleted taggables"
        );

        Ok(())
    }

    #[tracing::instrument(
        name = "tags.repository.resolve_taggable_tags",
        skip(self, tx, tags_by_taggable),
        fields(
            taggable_type = tracing::field::Empty,
            taggable_count = tracing::field::Empty,
            tag_name_count = tracing::field::Empty,
            resolved_pair_count = tracing::field::Empty
        ),
        err
    )]
    pub(crate) async fn resolve_taggable_tags<T>(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        tags_by_taggable: &[(T, SmallVec<[String; 3]>)],
    ) -> Result<SmallVec<[(TagUuid, T); 3]>, sqlx::Error>
    where
        T: Taggable + Copy,
    {
        tracing::Span::current().record("taggable_type", T::type_as_str());
        tracing::Span::current().record("taggable_count", tags_by_taggable.len());

        let all_names: SmallVec<[&str; 5]> = tags_by_taggable
            .iter()
            .flat_map(|(_, tags)| tags.iter().map(String::as_str))
            .collect();
        let tag_name_count = all_names.len();

        tracing::Span::current().record("tag_name_count", tag_name_count);

        let tag_map = self.sync_tags(tx, &all_names).await?;

        let resolved: SmallVec<[(TagUuid, T); 3]> = tags_by_taggable
            .iter()
            .flat_map(|(taggable_uuid, tag_names)| {
                tag_names.iter().filter_map(|name| {
                    tag_map
                        .get(name.as_str())
                        .copied()
                        .map(|tag_uuid| (tag_uuid, *taggable_uuid))
                })
            })
            .collect();

        let resolved_pair_count = resolved.len();

        tracing::Span::current().record("resolved_pair_count", resolved_pair_count);

        debug!(
            taggable_type = T::type_as_str(),
            taggable_count = tags_by_taggable.len(),
            tag_name_count,
            resolved_pair_count,
            "resolved taggable tags"
        );

        Ok(resolved)
    }

    #[cfg(test)]
    pub(crate) async fn list_taggable_tag_names<T>(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        taggable: T,
    ) -> Result<Vec<String>, sqlx::Error>
    where
        T: Taggable + Copy + Into<Uuid>,
    {
        sqlx::query_scalar::<Postgres, String>(LIST_TAGGABLE_TAG_NAMES_SQL)
            .bind(T::type_as_str())
            .bind(Into::<Uuid>::into(taggable))
            .fetch_all(&mut **tx)
            .await
    }
}
