use pgt_query::protobuf::ResTarget;

use crate::TokenKind;
use crate::emitter::{EventEmitter, GroupKind};

pub(super) fn emit_res_target(e: &mut EventEmitter, n: &ResTarget) {
    e.group_start(GroupKind::ResTarget);

    if let Some(ref val) = n.val {
        super::emit_node(val, e);
        if !n.name.is_empty() {
            e.space();
            e.token(TokenKind::AS_KW);
            e.space();
            e.token(TokenKind::IDENT(n.name.clone()));
        }
    } else if !n.name.is_empty() {
        e.token(TokenKind::IDENT(n.name.clone()));
    }

    e.group_end();
}
