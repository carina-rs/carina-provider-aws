use carina_core::schema::AttributeType;

use crate::{internet_gateway_id, vpn_gateway_id};

/// Gateway ID type: either an internet-gateway ID or a VPN-gateway ID.
pub fn gateway_id() -> AttributeType {
    AttributeType::union(vec![internet_gateway_id(), vpn_gateway_id()])
}
