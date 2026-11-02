use carina_core::schema::{AttributeType, StructField};

use crate::{internet_gateway_id, vpn_gateway_id};

/// Gateway ID type: either an internet-gateway ID or a VPN-gateway ID.
pub fn gateway_id() -> AttributeType {
    AttributeType::union(vec![internet_gateway_id(), vpn_gateway_id()])
}

/// Per-zone address settings for regional NAT gateways.
pub fn nat_gateway_availability_zone_addresses() -> AttributeType {
    AttributeType::unordered_list(AttributeType::struct_(
        "AvailabilityZoneAddress",
        vec![
            StructField::optional("allocation_ids", AttributeType::unordered_list(crate::allocation_id()))
                .with_description("The allocation IDs of the Elastic IP addresses (EIPs) to be used for handling outbound NAT traffic in this specific Availability Zone.")
                .with_provider_name("AllocationIds")
                .build(),
            StructField::optional("availability_zone", crate::availability_zone())
                .with_description("For regional NAT gateways only: The Availability Zone where this specific NAT gateway configuration will be active. Each AZ in a regional NAT gateway has its own configuration to handle outbound NAT traffic from that AZ.")
                .with_provider_name("AvailabilityZone")
                .build(),
        ],
    ))
}
