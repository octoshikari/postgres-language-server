mod column_ref;
mod node_list;
mod range_var;
mod res_target;
mod select_stmt;
mod string;

use column_ref::emit_column_ref;
use range_var::emit_range_var;
use res_target::emit_res_target;
use select_stmt::emit_select_stmt;
use string::emit_string;

use crate::emitter::EventEmitter;
use pgt_query::{NodeEnum, protobuf::Node};

pub fn emit_node(node: &Node, e: &mut EventEmitter) {
    if let Some(ref inner) = node.node {
        emit_node_enum(inner, e)
    }
}

pub fn emit_node_enum(node: &NodeEnum, e: &mut EventEmitter) {
    match &node {
        NodeEnum::SelectStmt(n) => emit_select_stmt(e, n),
        NodeEnum::ResTarget(n) => emit_res_target(e, n),
        NodeEnum::ColumnRef(n) => emit_column_ref(e, n),
        NodeEnum::String(n) => emit_string(e, n),
        NodeEnum::RangeVar(n) => emit_range_var(e, n),
        _ => todo!("emit_node_enum: unhandled node type {:?}", node),
    }
}
