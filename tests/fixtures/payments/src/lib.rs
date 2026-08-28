use specdrs::prelude::*;

mod stripe;

#[specdrs(span(id = "audit", entry = crate::charge))]
mod audit {}

#[specdrs(in_spans("checkout"))]
mod inline_member {
    pub fn inherited() {}
}

#[specdrs(
    span(
        id = "checkout",
        claims(
            Objectives(
                Job(
                    "Charge the customer and establish what we owe them." as purpose,
                ),
            ),
            Constraints(Time("Capture before recording." as operation_order)),
            NotApplicable(
                Resources = "No resource objective is part of the current contract.",
            ),
            evidence(
                operation_order(Test = crate::tests::charge_operation_order),
            ),
        )
    ),
    claims(
        Constraints(
            Interface(
                "The amount is denominated in the account currency." as accepts_amount,
            ),
        ),
    )
)]
pub fn charge(amount: u64) -> u64 {
    stripe::capture(amount)
}

#[cfg(test)]
mod tests {
    #[test]
    fn charge_operation_order() {}
}
