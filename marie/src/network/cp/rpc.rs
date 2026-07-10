use libp2p::request_response;
use serde::{Deserialize, Serialize};
use shared::id::ID;

use crate::model::declaration::ModelDeclaration;


#[derive(Debug, Serialize, Deserialize)]
pub struct RpcCall {
    id: ID,
    kind: RpcCallKind
}

#[derive(Debug, Serialize, Deserialize)]
pub enum RpcCallKind {
    GetModel(String)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RpcResult {
    id: ID,
    kind: RpcResultKind
}

#[derive(Debug, Serialize, Deserialize)]
pub enum RpcResultKind {
    GetModel(Option<ModelDeclaration>)
}

pub type Behaviour = request_response::json::Behaviour<RpcCall, RpcResult>;