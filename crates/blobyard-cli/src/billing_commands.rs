use clap::{Args, Subcommand, ValueEnum};

/// Billing operations.
#[derive(Clone, Debug, Subcommand)]
pub enum BillingCommand {
    /// Show the current plan, limits, and usage.
    Show,
    /// Create a hosted checkout session.
    Checkout(BillingCheckoutArgs),
    /// Create a hosted billing management session.
    Portal,
    /// Manage paid storage capacity.
    Storage {
        /// Storage billing operation.
        #[command(subcommand)]
        command: BillingStorageCommand,
    },
    /// Update the paid plan or Team seat count.
    Update(BillingCheckoutArgs),
}

/// Paid storage billing operations.
#[derive(Clone, Debug, Subcommand)]
pub enum BillingStorageCommand {
    /// Start checkout for the selected number of storage blobs.
    Checkout(StorageBillingArgs),
    /// Update the selected number of storage blobs.
    Update(StorageBillingArgs),
}

/// Paid plans accepted by hosted billing.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum BillingPlan {
    /// Individual paid plan.
    Solo,
    /// Multi-seat paid plan.
    Team,
}

impl BillingPlan {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Solo => "solo",
            Self::Team => "team",
        }
    }
}

/// Arguments for plan checkout and updates.
#[derive(Clone, Debug, Args)]
pub struct BillingCheckoutArgs {
    /// Paid plan to purchase or update.
    #[arg(value_enum)]
    pub plan: BillingPlan,
    /// Requested Team seat count.
    #[arg(long, value_name = "COUNT")]
    pub seats: Option<u16>,
}

/// Arguments for managed storage checkout and updates.
#[derive(Clone, Debug, Args)]
pub struct StorageBillingArgs {
    /// Target number of 100 GiB storage blobs.
    #[arg(value_name = "COUNT", value_parser = clap::value_parser!(u32).range(1..))]
    pub storage_blob_count: u32,
}
