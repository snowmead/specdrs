use specdrs::prelude::*;

specdrs_module!(in_spans("checkout"));

specdrs_span!(
    id = "ledger",
    parent = "checkout",
    entrypoint = self::capture,
    claims(
        Constraints(
            Invariants("Every capture is recorded exactly once." as single_record),
        ),
        evidence(
            single_record(Test = self::tests::positive_amount),
        ),
    ),
);

/// Capture an authorized payment.
#[specdrs(in_spans("audit"))]
#[specdrs(
    claims(
        Constraints(
            Invariants("A capture amount must be positive." as positive_amount),
        ),
        NotApplicable(
            Resources = "Capture resource use is not part of this item contract.",
        ),
        evidence(
            positive_amount(Test = crate::stripe::tests::positive_amount),
        ),
    )
)]
pub fn capture(amount: u64) -> u64 {
    amount
}

mod inherited_shapes {
    use specdrs::prelude::*;

    pub struct Gateway;

    #[specdrs(
        span(
            id = "gateway",
            parent = "checkout",
            entrypoint = self::Gateway::send,
            claims(
                Objectives(
                    Job("Move one authorized capture to the payment provider." as purpose),
                ),
                Constraints(
                    Interface(
                        "Every member takes the gateway by shared reference." as shared_reference_members,
                    ),
                ),
                evidence(
                    shared_reference_members(Test = crate::stripe::tests::gateway_members_share_the_gateway),
                ),
            ),
        )
    )]
    impl Gateway {
        #[specdrs(in_spans("audit", "checkout"))]
        pub fn send(&self) {}

        pub fn retry(&self) {}
    }

    pub trait Operation {
        fn execute(&self);
    }

    pub enum Status {
        Ready,
    }

    pub union Value {
        pub amount: u64,
    }

    pub type Amount = u64;
    pub const DEFAULT_AMOUNT: Amount = 1;
    pub static ENABLED: bool = true;
}

#[cfg(test)]
mod tests {
    #[test]
    fn positive_amount() {
        assert_eq!(super::capture(1), 1);
    }

    #[test]
    fn gateway_members_share_the_gateway() {
        super::inherited_shapes::Gateway.send();
        super::inherited_shapes::Gateway.retry();
    }
}
