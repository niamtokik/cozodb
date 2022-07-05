use crate::data::compare::{rusty_cmp, DB_KEY_PREFIX_LEN};
use crate::data::id::TxId;
use crate::runtime::transact::{SessionTx, TxLog};
use anyhow::Result;
use cozorocks::{DbBuilder, DbIter, RocksDb};
use std::fmt::{Debug, Formatter};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

pub struct Db {
    db: RocksDb,
    last_attr_id: Arc<AtomicU64>,
    last_ent_id: Arc<AtomicU64>,
    last_tx_id: Arc<AtomicU64>,
    n_sessions: Arc<AtomicUsize>,
    session_id: usize,
}

impl Debug for Db {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Db<session {}, attrs {:?}, entities {:?}, txs {:?}, sessions: {:?}>",
            self.session_id, self.last_tx_id, self.last_ent_id, self.last_tx_id, self.n_sessions
        )
    }
}

impl Db {
    pub fn build(builder: DbBuilder) -> Result<Self> {
        let db = builder
            .use_bloom_filter(true, 10., true)
            .use_capped_prefix_extractor(true, DB_KEY_PREFIX_LEN)
            .use_custom_comparator("cozo_rusty_cmp", rusty_cmp, false)
            .build()?;
        Ok(Self {
            db,
            last_attr_id: Arc::new(Default::default()),
            last_ent_id: Arc::new(Default::default()),
            last_tx_id: Arc::new(Default::default()),
            n_sessions: Arc::new(Default::default()),
            session_id: Default::default(),
        })
    }

    pub fn new_session(&self) -> Result<Self> {
        if self.session_id == 0 {
            self.load_last_ids()?;
        }

        let old_count = self.n_sessions.fetch_add(1, Ordering::AcqRel);

        Ok(Self {
            db: self.db.clone(),
            last_attr_id: self.last_attr_id.clone(),
            last_ent_id: self.last_ent_id.clone(),
            last_tx_id: self.last_tx_id.clone(),
            n_sessions: self.n_sessions.clone(),
            session_id: old_count + 1,
        })
    }

    fn load_last_ids(&self) -> Result<()> {
        let mut tx = self.transact(None)?;
        self.last_tx_id.store(tx.r_tx_id.0, Ordering::Release);
        self.last_attr_id
            .store(tx.load_last_attr_id()?.0, Ordering::Release);
        self.last_ent_id
            .store(tx.load_last_entity_id()?.0, Ordering::Release);
        Ok(())
    }
    pub(crate) fn transact(&self, at: Option<TxId>) -> Result<SessionTx> {
        let tx_id = at.unwrap_or(TxId(0));
        let mut ret = SessionTx {
            tx: self.db.transact().set_snapshot(true).start(),
            r_tx_id: tx_id,
            w_tx_id: None,
            last_attr_id: self.last_attr_id.clone(),
            last_ent_id: self.last_ent_id.clone(),
            last_tx_id: self.last_tx_id.clone(),
        };
        if at.is_none() {
            let tid = ret.load_last_tx_id()?;
            ret.r_tx_id = tid;
        }
        Ok(ret)
    }
    pub(crate) fn transact_write(&self) -> Result<SessionTx> {
        let last_tx_id = self.last_tx_id.fetch_add(1, Ordering::AcqRel);
        let cur_tx_id = TxId(last_tx_id + 1);

        let ret = SessionTx {
            tx: self.db.transact().set_snapshot(true).start(),
            r_tx_id: cur_tx_id,
            w_tx_id: Some(cur_tx_id),
            last_attr_id: self.last_attr_id.clone(),
            last_ent_id: self.last_ent_id.clone(),
            last_tx_id: self.last_tx_id.clone(),
        };
        Ok(ret)
    }
    pub(crate) fn total_iter(&self) -> DbIter {
        let mut it = self.db.transact().start().iterator().start();
        it.seek_to_start();
        it
    }
    pub(crate) fn find_tx_before_timestamp_millis(&self, ts: i64) -> Result<Option<TxLog>> {
        todo!()
    }
}
