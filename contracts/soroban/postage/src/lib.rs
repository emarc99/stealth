#![no_std]

use soroban_sdk::{
    contract, contractclient, contracterror, contractevent, contractimpl, contracttype,
    symbol_short, token, Address, BytesN, Env, MuxedAddress, Symbol,
};

#[contract]
pub struct PostageContract;

mod lifecycle_guard {
    use super::*;

    #[contracttype]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PolicyReason {
        SenderAllowed,
        SenderBlocked,
        UnknownSendersDisabled,
        VerificationRequired,
        ReceiptRequired,
        InsufficientPostage,
        PolicySatisfied,
        TierSatisfied,
    }

    #[contracttype]
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct Postage {
        pub sender: Address,
        pub recipient: Address,
        pub amount: i128,
        pub fee: i128,
        pub created_at: u64,
        pub expires_at: u64,
        pub dispute_until: u64,
        pub status: PostageStatus,
    }

    #[contracttype]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PostageStatus {
        Pending,
        Expired,
        Disputed,
        Settled,
        Refunded,
        Reclaimed,
    }

    #[contracttype]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum LifecycleTerminal {
        Open,
        Delivered,
        Read,
        Settled,
        Refunded,
        Disputed,
        Expired,
        Reclaimed,
    }

    #[contracttype]
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct LifecycleRecord {
        pub message_id: BytesN<32>,
        pub owner: Address,
        pub sender: Address,
        pub recipient: Address,
        pub amount: i128,
        pub verified: bool,
        pub receipt_required: bool,
        pub policy_version: u32,
        pub decision_reason: PolicyReason,
        pub payload_hash: Option<BytesN<32>>,
        pub protocol_version: Option<u32>,
        pub delivered_at: Option<u64>,
        pub read_at: Option<u64>,
        pub terminal: LifecycleTerminal,
        pub bound_at: u64,
    }

    #[contracterror]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    #[repr(u32)]
    pub enum LifecycleError {
        AlreadyInitialized = 1,
        NotInitialized = 2,
        UnauthorizedContract = 3,
        PolicyRejected = 4,
        PolicyVersionMismatch = 5,
        PostageMismatch = 6,
        ReceiptMismatch = 7,
        MissingLifecycle = 8,
        TerminalStateMismatch = 9,
        DuplicateLifecycle = 10,
        AlreadyDelivered = 11,
        AlreadyRead = 12,
    }

    #[contractclient(name = "LifecycleContractClient")]
    pub trait LifecycleContractInterface {
        fn verify_settle(
            message_id: BytesN<32>,
            postage: Postage,
        ) -> Result<LifecycleRecord, LifecycleError>;
        fn verify_refund(
            message_id: BytesN<32>,
            postage: Postage,
        ) -> Result<LifecycleRecord, LifecycleError>;
        fn verify_dispute(
            message_id: BytesN<32>,
            postage: Postage,
        ) -> Result<LifecycleRecord, LifecycleError>;
        fn verify_expire(
            message_id: BytesN<32>,
            postage: Postage,
        ) -> Result<LifecycleRecord, LifecycleError>;
        fn verify_reclaim(
            message_id: BytesN<32>,
            postage: Postage,
        ) -> Result<LifecycleRecord, LifecycleError>;
    }
}

use lifecycle_guard::{
    LifecycleContractClient, LifecycleTerminal, Postage as LifecyclePostage,
    PostageStatus as LifecyclePostageStatus,
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Postage {
    pub sender: Address,
    pub recipient: Address,
    pub amount: i128,
    pub fee: i128,
    pub created_at: u64,
    pub expires_at: u64,
    pub dispute_until: u64,
    pub status: PostageStatus,
}

#[contractevent(topics = ["postage"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostageEvent {
    #[topic]
    pub action: Symbol,
    #[topic]
    pub message_id: BytesN<32>,
    pub postage: Postage,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowConfig {
    pub asset: Address,
    pub minimum: i128,
    pub treasury: Address,
    pub fee_bps: u32,
    pub expiry_seconds: u64,
    pub dispute_seconds: u64,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PostageStatus {
    Pending,
    Expired,
    Disputed,
    Settled,
    Refunded,
    Reclaimed,
}

#[contracttype]
enum DataKey {
    Config,
    Guard,
    Postage(BytesN<32>),
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidAmount = 3,
    DuplicateMessage = 4,
    PostageNotFound = 5,
    AlreadyResolved = 6,
    InvalidFee = 7,
    InvalidWindow = 8,
    NotExpired = 9,
    DisputeUnavailable = 10,
    GuardNotConfigured = 11,
    LifecycleRejected = 12,
}

#[contractimpl]
impl PostageContract {
    pub fn initialize(
        env: Env,
        asset: Address,
        treasury: Address,
        minimum: i128,
        fee_bps: u32,
        expiry_seconds: u64,
        dispute_seconds: u64,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Config) {
            return Err(Error::AlreadyInitialized);
        }
        if minimum < 0 {
            return Err(Error::InvalidAmount);
        }
        if fee_bps > 10_000 {
            return Err(Error::InvalidFee);
        }
        if expiry_seconds == 0 {
            return Err(Error::InvalidWindow);
        }

        env.storage().instance().set(
            &DataKey::Config,
            &EscrowConfig {
                asset,
                minimum,
                treasury,
                fee_bps,
                expiry_seconds,
                dispute_seconds,
            },
        );
        Ok(())
    }

    pub fn configure_guard(env: Env, guard: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Guard) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Guard, &guard);
        Ok(())
    }

    pub fn guard(env: Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Guard)
            .ok_or(Error::GuardNotConfigured)
    }

    pub fn config(env: Env) -> Result<EscrowConfig, Error> {
        Self::read_config(&env)
    }

    pub fn minimum(env: Env) -> Result<i128, Error> {
        Ok(Self::read_config(&env)?.minimum)
    }

    pub fn quote(env: Env, sender_trusted: bool) -> Result<i128, Error> {
        if sender_trusted {
            return Ok(0);
        }
        Self::minimum(env)
    }

    pub fn submit(
        env: Env,
        message_id: BytesN<32>,
        sender: Address,
        recipient: Address,
        amount: i128,
    ) -> Result<Postage, Error> {
        sender.require_auth();

        let config = Self::read_config(&env)?;
        if amount < config.minimum {
            return Err(Error::InvalidAmount);
        }

        let key = DataKey::Postage(message_id.clone());
        if env.storage().persistent().has(&key) {
            return Err(Error::DuplicateMessage);
        }

        let fee = Self::fee_for(amount, config.fee_bps)?;
        let created_at = env.ledger().timestamp();
        let expires_at = Self::checked_deadline(created_at, config.expiry_seconds)?;
        let dispute_until = Self::checked_deadline(expires_at, config.dispute_seconds)?;

        token::TokenClient::new(&env, &config.asset).transfer(
            &sender,
            &MuxedAddress::from(env.current_contract_address()),
            &amount,
        );

        let postage = Postage {
            sender,
            recipient,
            amount,
            fee,
            created_at,
            expires_at,
            dispute_until,
            status: PostageStatus::Pending,
        };
        env.storage().persistent().set(&key, &postage);
        Self::publish_event(&env, symbol_short!("submit"), message_id, postage.clone());
        Ok(postage)
    }

    pub fn expire(env: Env, message_id: BytesN<32>) -> Result<Postage, Error> {
        let key = DataKey::Postage(message_id.clone());
        let mut postage: Postage = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::PostageNotFound)?;

        if Self::is_terminal(postage.status) {
            return Err(Error::AlreadyResolved);
        }
        if postage.status != PostageStatus::Pending {
            return Err(Error::DisputeUnavailable);
        }
        if env.ledger().timestamp() < postage.expires_at {
            return Err(Error::NotExpired);
        }

        Self::verify_guard(
            &env,
            message_id.clone(),
            &postage,
            LifecycleTerminal::Expired,
        )?;

        postage.status = PostageStatus::Expired;
        env.storage().persistent().set(&key, &postage);
        Self::publish_event(&env, symbol_short!("expire"), message_id, postage.clone());
        Ok(postage)
    }

    pub fn settle(env: Env, message_id: BytesN<32>) -> Result<Postage, Error> {
        let key = DataKey::Postage(message_id.clone());
        let postage: Postage = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::PostageNotFound)?;
        if Self::is_terminal(postage.status)
            || env.ledger().timestamp() >= Self::reclaimable_at(&postage)
        {
            return Err(Error::AlreadyResolved);
        }
        Self::verify_guard(
            &env,
            message_id.clone(),
            &postage,
            LifecycleTerminal::Settled,
        )?;
        Self::resolve(env, message_id, PostageStatus::Settled)
    }

    pub fn refund(env: Env, message_id: BytesN<32>) -> Result<Postage, Error> {
        let key = DataKey::Postage(message_id.clone());
        let postage: Postage = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::PostageNotFound)?;
        if Self::is_terminal(postage.status)
            || env.ledger().timestamp() >= Self::reclaimable_at(&postage)
        {
            return Err(Error::AlreadyResolved);
        }
        Self::verify_guard(
            &env,
            message_id.clone(),
            &postage,
            LifecycleTerminal::Refunded,
        )?;
        Self::resolve(env, message_id, PostageStatus::Refunded)
    }

    pub fn dispute(env: Env, message_id: BytesN<32>) -> Result<Postage, Error> {
        let key = DataKey::Postage(message_id.clone());
        let mut postage: Postage = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::PostageNotFound)?;

        postage.recipient.require_auth();
        if Self::is_terminal(postage.status) {
            return Err(Error::AlreadyResolved);
        }
        if !matches!(
            postage.status,
            PostageStatus::Pending | PostageStatus::Expired
        ) || postage.dispute_until == postage.expires_at
        {
            return Err(Error::DisputeUnavailable);
        }

        let now = env.ledger().timestamp();
        if now < postage.expires_at || now >= postage.dispute_until {
            return Err(Error::DisputeUnavailable);
        }

        Self::verify_guard(
            &env,
            message_id.clone(),
            &postage,
            LifecycleTerminal::Disputed,
        )?;

        postage.status = PostageStatus::Disputed;
        env.storage().persistent().set(&key, &postage);
        Self::publish_event(&env, symbol_short!("dispute"), message_id, postage.clone());
        Ok(postage)
    }

    pub fn reclaim(env: Env, message_id: BytesN<32>) -> Result<Postage, Error> {
        let key = DataKey::Postage(message_id.clone());
        let mut postage: Postage = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::PostageNotFound)?;

        postage.sender.require_auth();
        if Self::is_terminal(postage.status) {
            return Err(Error::AlreadyResolved);
        }

        let reclaimable_at = Self::reclaimable_at(&postage);
        if env.ledger().timestamp() < reclaimable_at {
            return Err(Error::NotExpired);
        }

        Self::verify_guard(
            &env,
            message_id.clone(),
            &postage,
            LifecycleTerminal::Reclaimed,
        )?;

        let config = Self::read_config(&env)?;
        token::TokenClient::new(&env, &config.asset).transfer(
            &env.current_contract_address(),
            &MuxedAddress::from(postage.sender.clone()),
            &postage.amount,
        );

        postage.status = PostageStatus::Reclaimed;
        env.storage().persistent().set(&key, &postage);
        Self::publish_event(&env, symbol_short!("reclaim"), message_id, postage.clone());
        Ok(postage)
    }

    pub fn get(env: Env, message_id: BytesN<32>) -> Result<Postage, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Postage(message_id))
            .ok_or(Error::PostageNotFound)
    }

    fn resolve(env: Env, message_id: BytesN<32>, status: PostageStatus) -> Result<Postage, Error> {
        let key = DataKey::Postage(message_id.clone());
        let mut postage: Postage = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::PostageNotFound)?;

        postage.recipient.require_auth();
        if Self::is_terminal(postage.status) {
            return Err(Error::AlreadyResolved);
        }
        if env.ledger().timestamp() >= Self::reclaimable_at(&postage) {
            return Err(Error::AlreadyResolved);
        }

        let config = Self::read_config(&env)?;
        let escrow = env.current_contract_address();
        let token = token::TokenClient::new(&env, &config.asset);
        match status {
            PostageStatus::Settled => {
                let recipient_amount = postage.amount - postage.fee;
                if recipient_amount > 0 {
                    token.transfer(
                        &escrow,
                        &MuxedAddress::from(postage.recipient.clone()),
                        &recipient_amount,
                    );
                }
                if postage.fee > 0 {
                    token.transfer(&escrow, &MuxedAddress::from(config.treasury), &postage.fee);
                }
            }
            PostageStatus::Refunded => {
                token.transfer(
                    &escrow,
                    &MuxedAddress::from(postage.sender.clone()),
                    &postage.amount,
                );
            }
            PostageStatus::Pending
            | PostageStatus::Expired
            | PostageStatus::Disputed
            | PostageStatus::Reclaimed => return Err(Error::AlreadyResolved),
        }

        postage.status = status;
        env.storage().persistent().set(&key, &postage);
        Self::publish_event(
            &env,
            Self::status_symbol(status),
            message_id,
            postage.clone(),
        );
        Ok(postage)
    }

    fn read_config(env: &Env) -> Result<EscrowConfig, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(Error::NotInitialized)
    }

    fn verify_guard(
        env: &Env,
        message_id: BytesN<32>,
        postage: &Postage,
        terminal: LifecycleTerminal,
    ) -> Result<(), Error> {
        let guard = env
            .storage()
            .instance()
            .get(&DataKey::Guard)
            .ok_or(Error::GuardNotConfigured)?;
        let lifecycle_postage = Self::to_lifecycle_postage(postage);
        let result = match terminal {
            LifecycleTerminal::Settled => LifecycleContractClient::new(env, &guard)
                .try_verify_settle(&message_id, &lifecycle_postage),
            LifecycleTerminal::Refunded => LifecycleContractClient::new(env, &guard)
                .try_verify_refund(&message_id, &lifecycle_postage),
            LifecycleTerminal::Disputed => LifecycleContractClient::new(env, &guard)
                .try_verify_dispute(&message_id, &lifecycle_postage),
            LifecycleTerminal::Expired => LifecycleContractClient::new(env, &guard)
                .try_verify_expire(&message_id, &lifecycle_postage),
            LifecycleTerminal::Reclaimed => LifecycleContractClient::new(env, &guard)
                .try_verify_reclaim(&message_id, &lifecycle_postage),
            LifecycleTerminal::Open | LifecycleTerminal::Delivered | LifecycleTerminal::Read => {
                return Err(Error::LifecycleRejected)
            }
        };

        match result {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(_)) | Err(_) => Err(Error::LifecycleRejected),
        }
    }

    fn to_lifecycle_postage(postage: &Postage) -> LifecyclePostage {
        LifecyclePostage {
            sender: postage.sender.clone(),
            recipient: postage.recipient.clone(),
            amount: postage.amount,
            fee: postage.fee,
            created_at: postage.created_at,
            expires_at: postage.expires_at,
            dispute_until: postage.dispute_until,
            status: match postage.status {
                PostageStatus::Pending => LifecyclePostageStatus::Pending,
                PostageStatus::Expired => LifecyclePostageStatus::Expired,
                PostageStatus::Disputed => LifecyclePostageStatus::Disputed,
                PostageStatus::Settled => LifecyclePostageStatus::Settled,
                PostageStatus::Refunded => LifecyclePostageStatus::Refunded,
                PostageStatus::Reclaimed => LifecyclePostageStatus::Reclaimed,
            },
        }
    }

    fn checked_deadline(timestamp: u64, seconds: u64) -> Result<u64, Error> {
        timestamp.checked_add(seconds).ok_or(Error::InvalidWindow)
    }

    fn is_terminal(status: PostageStatus) -> bool {
        matches!(
            status,
            PostageStatus::Settled | PostageStatus::Refunded | PostageStatus::Reclaimed
        )
    }

    fn reclaimable_at(postage: &Postage) -> u64 {
        if postage.dispute_until > postage.expires_at {
            postage.dispute_until
        } else {
            postage.expires_at
        }
    }

    fn publish_event(env: &Env, action: Symbol, message_id: BytesN<32>, postage: Postage) {
        PostageEvent {
            action,
            message_id,
            postage,
        }
        .publish(env);
    }

    fn status_symbol(status: PostageStatus) -> Symbol {
        match status {
            PostageStatus::Settled => symbol_short!("settle"),
            PostageStatus::Refunded => symbol_short!("refund"),
            PostageStatus::Reclaimed => symbol_short!("reclaim"),
            PostageStatus::Expired => symbol_short!("expire"),
            PostageStatus::Disputed => symbol_short!("dispute"),
            PostageStatus::Pending => symbol_short!("pending"),
        }
    }

    fn fee_for(amount: i128, fee_bps: u32) -> Result<i128, Error> {
        if amount < 0 {
            return Err(Error::InvalidAmount);
        }
        amount
            .checked_mul(fee_bps as i128)
            .and_then(|gross| gross.checked_div(10_000))
            .ok_or(Error::InvalidAmount)
    }
}

#[cfg(test)]
mod test {
    extern crate std;

    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, AuthorizedFunction, AuthorizedInvocation, Events, Ledger},
        Event, IntoVal,
    };
    use stealth_lifecycle::LifecycleContract;
    use stealth_lifecycle::LifecycleContractClient;
    use stealth_policies::PoliciesContract;
    use stealth_policies::{MailboxPolicy, PoliciesContractClient};

    fn id(env: &Env, byte: u8) -> BytesN<32> {
        BytesN::from_array(env, &[byte; 32])
    }

    fn bind_lifecycle(
        env: &Env,
        lifecycle: &Address,
        message_id: BytesN<32>,
        sender: &Address,
        recipient: &Address,
        amount: i128,
    ) {
        let lifecycle_client = LifecycleContractClient::new(env, lifecycle);
        lifecycle_client.bind(
            &message_id,
            &recipient.clone(),
            &sender.clone(),
            &recipient.clone(),
            &amount,
            &false,
            &false,
        );
    }

    struct Setup {
        env: Env,
        contract_id: Address,
        asset: Address,
        sender: Address,
        recipient: Address,
        treasury: Address,
        lifecycle: Address,
        policies: Address,
        receipts: Address,
    }

    fn setup(fee_bps: u32) -> Setup {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(42);
        env.ledger().set_sequence_number(10);
        let admin = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract_v2(admin.clone());
        let asset = token_contract.address();
        let token_admin = token::StellarAssetClient::new(&env, &asset);

        // Set up policies contract with permissive default policy
        let policies = env.register(PoliciesContract, ());
        let policies_client = PoliciesContractClient::new(&env, &policies);

        let sender = Address::generate(&env);
        let recipient = Address::generate(&env);
        let treasury = Address::generate(&env);

        policies_client.set_policy(
            &recipient.clone(),
            &MailboxPolicy {
                allow_unknown: true,
                require_verified: false,
                require_receipt: false,
                minimum_postage: 0,
            },
        );

        // Set up lifecycle contract - will be initialized after postage contract is created
        let receipts = Address::generate(&env);
        let lifecycle = env.register(LifecycleContract, ());
        let lifecycle_client = LifecycleContractClient::new(&env, &lifecycle);

        let contract_id = env.register(PostageContract, ());
        let client = PostageContractClient::new(&env, &contract_id);

        token_admin.mint(&sender, &1_000);
        client.initialize(&asset, &treasury, &100, &fee_bps, &86_400, &3_600);
        client.configure_guard(&lifecycle);

        // Now initialize lifecycle with the actual postage contract address
        lifecycle_client.initialize(&policies, &contract_id, &receipts);

        Setup {
            env,
            contract_id,
            asset,
            sender,
            recipient,
            treasury,
            lifecycle,
            policies,
            receipts,
        }
    }

    #[test]
    fn records_escrows_and_settles_postage() {
        let setup = setup(500);
        let client = PostageContractClient::new(&setup.env, &setup.contract_id);
        let token = token::TokenClient::new(&setup.env, &setup.asset);

        let postage = client.submit(&id(&setup.env, 1), &setup.sender, &setup.recipient, &200);
        assert_eq!(postage.status, PostageStatus::Pending);
        assert_eq!(postage.created_at, 42);
        assert_eq!(postage.expires_at, 86_442);
        assert_eq!(postage.dispute_until, 90_042);
        assert_eq!(postage.fee, 10);
        assert_eq!(token.balance(&setup.sender), 800);
        assert_eq!(token.balance(&setup.contract_id), 200);

        bind_lifecycle(
            &setup.env,
            &setup.lifecycle,
            id(&setup.env, 1),
            &setup.sender,
            &setup.recipient,
            200,
        );

        let settled = client.settle(&id(&setup.env, 1));
        assert_eq!(settled.status, PostageStatus::Settled);
        assert_eq!(token.balance(&setup.contract_id), 0);
        assert_eq!(token.balance(&setup.recipient), 190);
        assert_eq!(token.balance(&setup.treasury), 10);
        assert_eq!(
            token.balance(&setup.sender)
                + token.balance(&setup.recipient)
                + token.balance(&setup.treasury)
                + token.balance(&setup.contract_id),
            1_000
        );
    }

    #[test]
    fn refund_returns_full_escrow_to_sender() {
        let setup = setup(250);
        let client = PostageContractClient::new(&setup.env, &setup.contract_id);
        let token = token::TokenClient::new(&setup.env, &setup.asset);

        client.submit(&id(&setup.env, 1), &setup.sender, &setup.recipient, &200);
        bind_lifecycle(
            &setup.env,
            &setup.lifecycle,
            id(&setup.env, 1),
            &setup.sender,
            &setup.recipient,
            200,
        );
        let refunded = client.refund(&id(&setup.env, 1));

        assert_eq!(refunded.status, PostageStatus::Refunded);
        assert_eq!(token.balance(&setup.sender), 1_000);
        assert_eq!(token.balance(&setup.recipient), 0);
        assert_eq!(token.balance(&setup.treasury), 0);
        assert_eq!(token.balance(&setup.contract_id), 0);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #6)")]
    fn double_settlement_and_refund_are_impossible() {
        let setup = setup(0);
        let client = PostageContractClient::new(&setup.env, &setup.contract_id);

        client.submit(&id(&setup.env, 1), &setup.sender, &setup.recipient, &125);
        bind_lifecycle(
            &setup.env,
            &setup.lifecycle,
            id(&setup.env, 1),
            &setup.sender,
            &setup.recipient,
            125,
        );
        client.settle(&id(&setup.env, 1));
        client.refund(&id(&setup.env, 1));
    }

    #[test]
    fn accepted_asset_and_fee_policy_are_explicit() {
        let setup = setup(125);
        let client = PostageContractClient::new(&setup.env, &setup.contract_id);

        assert_eq!(
            client.config(),
            EscrowConfig {
                asset: setup.asset,
                minimum: 100,
                treasury: setup.treasury,
                fee_bps: 125,
                expiry_seconds: 86_400,
                dispute_seconds: 3_600,
            }
        );
    }

    #[test]
    fn trusted_sender_has_zero_quote() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let asset = env.register_stellar_asset_contract_v2(admin).address();
        let treasury = Address::generate(&env);
        let contract_id = env.register(PostageContract, ());
        let client = PostageContractClient::new(&env, &contract_id);
        client.initialize(&asset, &treasury, &100, &0, &86_400, &0);

        assert_eq!(client.quote(&true), 0);
        assert_eq!(client.quote(&false), 100);
    }

    #[test]
    fn authorization_tree_captures_sender_deposit_and_recipient_resolution() {
        let setup = setup(0);
        let client = PostageContractClient::new(&setup.env, &setup.contract_id);

        client.submit(&id(&setup.env, 1), &setup.sender, &setup.recipient, &125);
        assert_eq!(
            setup.env.auths(),
            [(
                setup.sender.clone(),
                AuthorizedInvocation {
                    function: AuthorizedFunction::Contract((
                        setup.contract_id.clone(),
                        symbol_short!("submit"),
                        (
                            id(&setup.env, 1),
                            setup.sender.clone(),
                            setup.recipient.clone(),
                            125_i128,
                        )
                            .into_val(&setup.env),
                    )),
                    sub_invocations: [AuthorizedInvocation {
                        function: AuthorizedFunction::Contract((
                            setup.asset.clone(),
                            symbol_short!("transfer"),
                            (
                                setup.sender.clone(),
                                MuxedAddress::from(setup.contract_id.clone()),
                                125_i128,
                            )
                                .into_val(&setup.env),
                        )),
                        sub_invocations: [].into(),
                    }]
                    .into(),
                }
            )]
        );

        bind_lifecycle(
            &setup.env,
            &setup.lifecycle,
            id(&setup.env, 1),
            &setup.sender,
            &setup.recipient,
            125,
        );
        client.settle(&id(&setup.env, 1));
        assert_eq!(
            setup.env.auths(),
            [(
                setup.recipient.clone(),
                AuthorizedInvocation {
                    function: AuthorizedFunction::Contract((
                        setup.contract_id.clone(),
                        symbol_short!("settle"),
                        (id(&setup.env, 1),).into_val(&setup.env),
                    )),
                    sub_invocations: [].into(),
                }
            )]
        );
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #9)")]
    fn reclaim_fails_before_expiry() {
        let setup = setup(0);
        let client = PostageContractClient::new(&setup.env, &setup.contract_id);

        client.submit(&id(&setup.env, 1), &setup.sender, &setup.recipient, &125);
        setup.env.ledger().set_timestamp(86_441);
        client.reclaim(&id(&setup.env, 1));
    }

    #[test]
    fn reclaim_succeeds_at_expiry_when_dispute_window_is_disabled() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(10);
        let admin = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract_v2(admin.clone());
        let asset = token_contract.address();
        let token_admin = token::StellarAssetClient::new(&env, &asset);

        // Set up policies and lifecycle
        let policies = env.register(PoliciesContract, ());
        let policies_client = PoliciesContractClient::new(&env, &policies);
        let sender = Address::generate(&env);
        let recipient = Address::generate(&env);
        policies_client.set_policy(
            &recipient.clone(),
            &MailboxPolicy {
                allow_unknown: true,
                require_verified: false,
                require_receipt: false,
                minimum_postage: 0,
            },
        );
        let receipts = Address::generate(&env);
        let lifecycle = env.register(LifecycleContract, ());
        let lifecycle_client = LifecycleContractClient::new(&env, &lifecycle);

        let contract_id = env.register(PostageContract, ());
        let client = PostageContractClient::new(&env, &contract_id);
        let treasury = Address::generate(&env);
        let token = token::TokenClient::new(&env, &asset);

        token_admin.mint(&sender, &1_000);
        client.initialize(&asset, &treasury, &100, &0, &30, &0);
        client.configure_guard(&lifecycle);
        lifecycle_client.initialize(&policies, &contract_id, &receipts);

        let postage = client.submit(&id(&env, 1), &sender, &recipient, &125);
        assert_eq!(postage.expires_at, 40);
        assert_eq!(postage.dispute_until, 40);
        bind_lifecycle(&env, &lifecycle, id(&env, 1), &sender, &recipient, 125);

        env.ledger().set_timestamp(40);
        let reclaimed = client.reclaim(&id(&env, 1));

        assert_eq!(reclaimed.status, PostageStatus::Reclaimed);
        assert_eq!(token.balance(&sender), 1_000);
        assert_eq!(token.balance(&contract_id), 0);
    }

    #[test]
    fn expiry_state_is_fixed_and_callable_at_boundary() {
        let setup = setup(0);
        let client = PostageContractClient::new(&setup.env, &setup.contract_id);

        let postage = client.submit(&id(&setup.env, 1), &setup.sender, &setup.recipient, &125);
        assert_eq!(postage.expires_at, 86_442);

        bind_lifecycle(
            &setup.env,
            &setup.lifecycle,
            id(&setup.env, 1),
            &setup.sender,
            &setup.recipient,
            125,
        );

        setup.env.ledger().set_timestamp(86_441);
        assert_eq!(
            client.try_expire(&id(&setup.env, 1)),
            Err(Ok(Error::NotExpired))
        );

        setup.env.ledger().set_timestamp(86_442);
        let expired = client.expire(&id(&setup.env, 1));
        assert_eq!(expired.status, PostageStatus::Expired);
        assert_eq!(expired.expires_at, 86_442);
        assert_eq!(expired.dispute_until, 90_042);
    }

    #[test]
    fn dispute_window_blocks_reclaim_until_boundary() {
        let setup = setup(0);
        let client = PostageContractClient::new(&setup.env, &setup.contract_id);
        let token = token::TokenClient::new(&setup.env, &setup.asset);

        client.submit(&id(&setup.env, 1), &setup.sender, &setup.recipient, &125);
        bind_lifecycle(
            &setup.env,
            &setup.lifecycle,
            id(&setup.env, 1),
            &setup.sender,
            &setup.recipient,
            125,
        );
        setup.env.ledger().set_timestamp(86_442);
        let disputed = client.dispute(&id(&setup.env, 1));
        assert_eq!(disputed.status, PostageStatus::Disputed);

        setup.env.ledger().set_timestamp(90_041);
        assert_eq!(
            client.try_reclaim(&id(&setup.env, 1)),
            Err(Ok(Error::NotExpired))
        );

        setup.env.ledger().set_timestamp(90_042);
        let reclaimed = client.reclaim(&id(&setup.env, 1));
        assert_eq!(reclaimed.status, PostageStatus::Reclaimed);
        assert_eq!(token.balance(&setup.sender), 1_000);
        assert_eq!(token.balance(&setup.contract_id), 0);
    }

    #[test]
    fn expired_postage_can_be_disputed_or_reclaimed() {
        let setup = setup(0);
        let client = PostageContractClient::new(&setup.env, &setup.contract_id);
        let token = token::TokenClient::new(&setup.env, &setup.asset);

        client.submit(&id(&setup.env, 1), &setup.sender, &setup.recipient, &125);
        client.submit(&id(&setup.env, 2), &setup.sender, &setup.recipient, &125);

        bind_lifecycle(
            &setup.env,
            &setup.lifecycle,
            id(&setup.env, 1),
            &setup.sender,
            &setup.recipient,
            125,
        );
        bind_lifecycle(
            &setup.env,
            &setup.lifecycle,
            id(&setup.env, 2),
            &setup.sender,
            &setup.recipient,
            125,
        );

        setup.env.ledger().set_timestamp(86_442);
        client.expire(&id(&setup.env, 1));
        let disputed = client.dispute(&id(&setup.env, 1));
        assert_eq!(disputed.status, PostageStatus::Disputed);

        setup.env.ledger().set_timestamp(90_042);
        client.expire(&id(&setup.env, 2));
        let reclaimed = client.reclaim(&id(&setup.env, 2));
        assert_eq!(reclaimed.status, PostageStatus::Reclaimed);
        assert_eq!(token.balance(&setup.contract_id), 125);
    }

    #[test]
    fn expiry_and_reclaim_emit_typed_events() {
        let setup = setup(0);
        let client = PostageContractClient::new(&setup.env, &setup.contract_id);
        let message_id = id(&setup.env, 1);

        client.submit(&message_id, &setup.sender, &setup.recipient, &125);
        bind_lifecycle(
            &setup.env,
            &setup.lifecycle,
            message_id.clone(),
            &setup.sender,
            &setup.recipient,
            125,
        );

        setup.env.ledger().set_timestamp(90_042);

        let expired = client.expire(&message_id);
        assert_eq!(
            setup
                .env
                .events()
                .all()
                .filter_by_contract(&setup.contract_id),
            std::vec![PostageEvent {
                action: symbol_short!("expire"),
                message_id: message_id.clone(),
                postage: expired,
            }
            .to_xdr(&setup.env, &setup.contract_id)]
        );

        let reclaimed = client.reclaim(&message_id);
        assert_eq!(
            setup
                .env
                .events()
                .all()
                .filter_by_contract(&setup.contract_id),
            std::vec![PostageEvent {
                action: symbol_short!("reclaim"),
                message_id,
                postage: reclaimed,
            }
            .to_xdr(&setup.env, &setup.contract_id)]
        );
    }

    #[test]
    fn dispute_fails_at_dispute_deadline() {
        let setup = setup(0);
        let client = PostageContractClient::new(&setup.env, &setup.contract_id);

        client.submit(&id(&setup.env, 1), &setup.sender, &setup.recipient, &125);
        bind_lifecycle(
            &setup.env,
            &setup.lifecycle,
            id(&setup.env, 1),
            &setup.sender,
            &setup.recipient,
            125,
        );
        setup.env.ledger().set_timestamp(90_042);

        assert_eq!(
            client.try_dispute(&id(&setup.env, 1)),
            Err(Ok(Error::DisputeUnavailable))
        );
    }

    #[test]
    fn disputed_postage_can_be_refunded_before_deadline() {
        let setup = setup(0);
        let client = PostageContractClient::new(&setup.env, &setup.contract_id);
        let token = token::TokenClient::new(&setup.env, &setup.asset);

        client.submit(&id(&setup.env, 1), &setup.sender, &setup.recipient, &125);
        bind_lifecycle(
            &setup.env,
            &setup.lifecycle,
            id(&setup.env, 1),
            &setup.sender,
            &setup.recipient,
            125,
        );
        setup.env.ledger().set_timestamp(86_442);
        client.dispute(&id(&setup.env, 1));
        setup.env.ledger().set_timestamp(90_041);
        let refunded = client.refund(&id(&setup.env, 1));

        assert_eq!(refunded.status, PostageStatus::Refunded);
        assert_eq!(token.balance(&setup.sender), 1_000);
        assert_eq!(token.balance(&setup.contract_id), 0);
    }

    #[test]
    fn recipient_resolution_fails_at_reclaim_boundary() {
        let setup = setup(0);
        let client = PostageContractClient::new(&setup.env, &setup.contract_id);

        client.submit(&id(&setup.env, 1), &setup.sender, &setup.recipient, &125);
        client.submit(&id(&setup.env, 2), &setup.sender, &setup.recipient, &125);
        bind_lifecycle(
            &setup.env,
            &setup.lifecycle,
            id(&setup.env, 1),
            &setup.sender,
            &setup.recipient,
            125,
        );
        bind_lifecycle(
            &setup.env,
            &setup.lifecycle,
            id(&setup.env, 2),
            &setup.sender,
            &setup.recipient,
            125,
        );
        setup.env.ledger().set_timestamp(90_041);
        assert_eq!(
            client.settle(&id(&setup.env, 1)).status,
            PostageStatus::Settled
        );

        setup.env.ledger().set_timestamp(90_042);
        assert_eq!(
            client.try_refund(&id(&setup.env, 2)),
            Err(Ok(Error::AlreadyResolved))
        );
        assert_eq!(
            client.try_settle(&id(&setup.env, 2)),
            Err(Ok(Error::AlreadyResolved))
        );
    }

    #[test]
    fn terminal_states_cannot_transition() {
        let setup = setup(0);
        let client = PostageContractClient::new(&setup.env, &setup.contract_id);

        client.submit(&id(&setup.env, 1), &setup.sender, &setup.recipient, &125);
        bind_lifecycle(
            &setup.env,
            &setup.lifecycle,
            id(&setup.env, 1),
            &setup.sender,
            &setup.recipient,
            125,
        );
        setup.env.ledger().set_timestamp(90_042);
        client.reclaim(&id(&setup.env, 1));

        assert_eq!(
            client.try_settle(&id(&setup.env, 1)),
            Err(Ok(Error::AlreadyResolved))
        );
        assert_eq!(
            client.try_refund(&id(&setup.env, 1)),
            Err(Ok(Error::AlreadyResolved))
        );
        assert_eq!(
            client.try_dispute(&id(&setup.env, 1)),
            Err(Ok(Error::AlreadyResolved))
        );
        assert_eq!(
            client.try_expire(&id(&setup.env, 1)),
            Err(Ok(Error::AlreadyResolved))
        );

        client.submit(&id(&setup.env, 2), &setup.sender, &setup.recipient, &125);
        bind_lifecycle(
            &setup.env,
            &setup.lifecycle,
            id(&setup.env, 2),
            &setup.sender,
            &setup.recipient,
            125,
        );
        client.refund(&id(&setup.env, 2));
        assert_eq!(
            client.try_reclaim(&id(&setup.env, 2)),
            Err(Ok(Error::AlreadyResolved))
        );

        client.submit(&id(&setup.env, 3), &setup.sender, &setup.recipient, &125);
        bind_lifecycle(
            &setup.env,
            &setup.lifecycle,
            id(&setup.env, 3),
            &setup.sender,
            &setup.recipient,
            125,
        );
        client.settle(&id(&setup.env, 3));
        assert_eq!(
            client.try_dispute(&id(&setup.env, 3)),
            Err(Ok(Error::AlreadyResolved))
        );
    }

    #[test]
    #[should_panic(expected = "Error(Auth")]
    fn submit_requires_sender_auth() {
        let setup = setup(0);
        let client = PostageContractClient::new(&setup.env, &setup.contract_id);
        let wrong_address = Address::generate(&setup.env);

        setup.env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &wrong_address,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &setup.contract_id,
                fn_name: "submit",
                args: (
                    id(&setup.env, 1),
                    setup.sender.clone(),
                    setup.recipient.clone(),
                    125_i128,
                )
                    .into_val(&setup.env),
                sub_invokes: &[],
            },
        }]);

        client.submit(&id(&setup.env, 1), &setup.sender, &setup.recipient, &125);
    }

    #[test]
    #[should_panic(expected = "Error(Auth")]
    fn dispute_requires_recipient_auth() {
        let setup = setup(0);
        let client = PostageContractClient::new(&setup.env, &setup.contract_id);
        client.submit(&id(&setup.env, 1), &setup.sender, &setup.recipient, &125);
        bind_lifecycle(
            &setup.env,
            &setup.lifecycle,
            id(&setup.env, 1),
            &setup.sender,
            &setup.recipient,
            125,
        );
        setup.env.ledger().set_timestamp(86_442);

        let wrong_address = Address::generate(&setup.env);
        setup.env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &wrong_address,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &setup.contract_id,
                fn_name: "dispute",
                args: (id(&setup.env, 1),).into_val(&setup.env),
                sub_invokes: &[],
            },
        }]);

        client.dispute(&id(&setup.env, 1));
    }

    #[test]
    #[should_panic(expected = "Error(Auth")]
    fn settle_requires_recipient_auth() {
        let setup = setup(0);
        let client = PostageContractClient::new(&setup.env, &setup.contract_id);
        client.submit(&id(&setup.env, 1), &setup.sender, &setup.recipient, &125);
        bind_lifecycle(
            &setup.env,
            &setup.lifecycle,
            id(&setup.env, 1),
            &setup.sender,
            &setup.recipient,
            125,
        );

        let wrong_address = Address::generate(&setup.env);
        setup.env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &wrong_address,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &setup.contract_id,
                fn_name: "settle",
                args: (id(&setup.env, 1),).into_val(&setup.env),
                sub_invokes: &[],
            },
        }]);

        client.settle(&id(&setup.env, 1));
    }

    #[test]
    #[should_panic(expected = "Error(Auth")]
    fn refund_requires_recipient_auth() {
        let setup = setup(0);
        let client = PostageContractClient::new(&setup.env, &setup.contract_id);
        client.submit(&id(&setup.env, 1), &setup.sender, &setup.recipient, &125);
        bind_lifecycle(
            &setup.env,
            &setup.lifecycle,
            id(&setup.env, 1),
            &setup.sender,
            &setup.recipient,
            125,
        );

        let wrong_address = Address::generate(&setup.env);
        setup.env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &wrong_address,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &setup.contract_id,
                fn_name: "refund",
                args: (id(&setup.env, 1),).into_val(&setup.env),
                sub_invokes: &[],
            },
        }]);

        client.refund(&id(&setup.env, 1));
    }

    #[test]
    #[should_panic(expected = "Error(Auth")]
    fn reclaim_requires_sender_auth() {
        let setup = setup(0);
        let client = PostageContractClient::new(&setup.env, &setup.contract_id);
        client.submit(&id(&setup.env, 1), &setup.sender, &setup.recipient, &125);
        setup.env.ledger().set_timestamp(90_042);

        let wrong_address = Address::generate(&setup.env);
        setup.env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &wrong_address,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &setup.contract_id,
                fn_name: "reclaim",
                args: (id(&setup.env, 1),).into_val(&setup.env),
                sub_invokes: &[],
            },
        }]);

        client.reclaim(&id(&setup.env, 1));
    }

    #[test]
    fn expire_has_no_auth_requirement() {
        let setup = setup(0);
        let client = PostageContractClient::new(&setup.env, &setup.contract_id);
        client.submit(&id(&setup.env, 1), &setup.sender, &setup.recipient, &125);
        bind_lifecycle(
            &setup.env,
            &setup.lifecycle,
            id(&setup.env, 1),
            &setup.sender,
            &setup.recipient,
            125,
        );
        setup.env.ledger().set_timestamp(86_442);

        setup.env.mock_auths(&[]);

        let expired = client.expire(&id(&setup.env, 1));
        assert_eq!(expired.status, PostageStatus::Expired);
    }
}

#[cfg(test)]
mod spec_check {
    // Contract spec regeneration check.
    //
    // spec.json feeds scripts/generate-contract-bindings.mjs, which emits the
    // typed TypeScript clients used against the ledger. If the contract
    // interface changes without regenerating spec.json, the bindings silently
    // drift from on-chain reality. This module decodes the XDR spec entries
    // that the soroban-sdk macros embed in the crate — the same entries a wasm
    // build publishes in its contractspecv0 section — renders the canonical
    // spec.json from them, and fails if the committed file differs.
    //
    // To regenerate after an interface change:
    //   UPDATE_SPEC=1 cargo test -p stealth-postage spec_json
    extern crate std;

    use std::format;
    use std::string::{String, ToString};
    use std::vec::Vec;

    use soroban_sdk::xdr::{Limits, ReadXdr, ScSpecEntry, ScSpecTypeDef, ScSpecUdtUnionCaseV0};

    use super::{Error, EscrowConfig, Postage, PostageContract, PostageStatus};

    const SPEC_JSON: &str = include_str!("../spec.json");
    const LIB_RS: &str = include_str!("lib.rs");

    /// Every spec entry the contract exports, in canonical spec.json order.
    /// Adding a public contract function requires adding its entry here; the
    /// `spec_covers_every_public_contract_function` test enforces that.
    fn entries() -> Vec<ScSpecEntry> {
        let xdrs: Vec<Vec<u8>> = std::vec![
            Postage::spec_xdr().to_vec(),
            EscrowConfig::spec_xdr().to_vec(),
            PostageStatus::spec_xdr().to_vec(),
            Error::spec_xdr().to_vec(),
            PostageContract::spec_xdr_initialize().to_vec(),
            PostageContract::spec_xdr_configure_guard().to_vec(),
            PostageContract::spec_xdr_guard().to_vec(),
            PostageContract::spec_xdr_config().to_vec(),
            PostageContract::spec_xdr_minimum().to_vec(),
            PostageContract::spec_xdr_quote().to_vec(),
            PostageContract::spec_xdr_submit().to_vec(),
            PostageContract::spec_xdr_settle().to_vec(),
            PostageContract::spec_xdr_refund().to_vec(),
            PostageContract::spec_xdr_dispute().to_vec(),
            PostageContract::spec_xdr_expire().to_vec(),
            PostageContract::spec_xdr_reclaim().to_vec(),
            PostageContract::spec_xdr_get().to_vec(),
        ];
        xdrs.iter()
            .map(|xdr| {
                ScSpecEntry::from_xdr(xdr.as_slice(), Limits::none())
                    .expect("embedded contract spec entry must decode")
            })
            .collect()
    }

    /// Render a type using the grammar consumed by
    /// scripts/generate-contract-bindings.mjs.
    fn render_type(def: &ScSpecTypeDef) -> String {
        match def {
            ScSpecTypeDef::Void => "void".to_string(),
            ScSpecTypeDef::Bool => "bool".to_string(),
            ScSpecTypeDef::U32 => "u32".to_string(),
            ScSpecTypeDef::I32 => "i32".to_string(),
            ScSpecTypeDef::U64 => "u64".to_string(),
            ScSpecTypeDef::I64 => "i64".to_string(),
            ScSpecTypeDef::U128 => "u128".to_string(),
            ScSpecTypeDef::I128 => "i128".to_string(),
            ScSpecTypeDef::Address => "address".to_string(),
            ScSpecTypeDef::BytesN(b) if b.n == 32 => "bytes32".to_string(),
            ScSpecTypeDef::Option(o) => format!("option:{}", render_type(&o.value_type)),
            ScSpecTypeDef::Udt(u) => format!("udt:{}", u.name.to_utf8_string_lossy()),
            ScSpecTypeDef::Result(r) => {
                // Contract errors appear as the built-in error type in XDR;
                // this crate has exactly one #[contracterror] enum, `Error`.
                let err = match &*r.error_type {
                    ScSpecTypeDef::Error => "Error".to_string(),
                    ScSpecTypeDef::Udt(u) => u.name.to_utf8_string_lossy(),
                    other => std::panic!("unsupported error type in spec: {other:?}"),
                };
                format!("result:{}:{}", render_type(&r.ok_type), err)
            }
            other => std::panic!("type not covered by the spec.json grammar: {other:?}"),
        }
    }

    fn render_name_type_list(items: &[(String, String)], indent: &str) -> String {
        let rendered: Vec<String> = items
            .iter()
            .map(|(name, ty)| format!("{{ \"name\": \"{name}\", \"type\": \"{ty}\" }}"))
            .collect();
        render_array(&rendered, indent)
    }

    fn render_case_list(items: &[(String, u32)], indent: &str) -> String {
        let rendered: Vec<String> = items
            .iter()
            .map(|(name, value)| format!("{{ \"name\": \"{name}\", \"value\": {value} }}"))
            .collect();
        render_array(&rendered, indent)
    }

    /// Arrays with zero or one element stay inline; longer arrays go one
    /// element per line, matching the committed spec.json style.
    fn render_array(rendered: &[String], indent: &str) -> String {
        match rendered {
            [] => "[]".to_string(),
            [only] if !only.contains('\n') => format!("[{only}]"),
            many => {
                let inner = many
                    .iter()
                    .map(|item| format!("{indent}  {item}"))
                    .collect::<Vec<_>>()
                    .join(",\n");
                format!("[\n{inner}\n{indent}]")
            }
        }
    }

    /// Render the canonical spec.json for the current contract interface.
    fn render_spec_json() -> String {
        let mut structs: Vec<String> = Vec::new();
        let mut enums: Vec<String> = Vec::new();
        let mut errors: Vec<(String, u32)> = Vec::new();
        let mut functions: Vec<String> = Vec::new();

        for entry in entries() {
            match entry {
                ScSpecEntry::UdtStructV0(s) => {
                    let fields: Vec<(String, String)> = s
                        .fields
                        .iter()
                        .map(|f| (f.name.to_utf8_string_lossy(), render_type(&f.type_)))
                        .collect();
                    structs.push(format!(
                        "{{\n      \"name\": \"{}\",\n      \"fields\": {}\n    }}",
                        s.name.to_utf8_string_lossy(),
                        render_name_type_list(&fields, "      "),
                    ));
                }
                ScSpecEntry::UdtUnionV0(u) => {
                    let cases: Vec<(String, u32)> = u
                        .cases
                        .iter()
                        .enumerate()
                        .map(|(index, case)| match case {
                            ScSpecUdtUnionCaseV0::VoidV0(v) => {
                                (v.name.to_utf8_string_lossy(), index as u32)
                            }
                            ScSpecUdtUnionCaseV0::TupleV0(t) => std::panic!(
                                "tuple union case {} is not covered by the spec.json grammar",
                                t.name.to_utf8_string_lossy()
                            ),
                        })
                        .collect();
                    enums.push(format!(
                        "{{\n      \"name\": \"{}\",\n      \"cases\": {}\n    }}",
                        u.name.to_utf8_string_lossy(),
                        render_case_list(&cases, "      "),
                    ));
                }
                ScSpecEntry::UdtErrorEnumV0(e) => {
                    for case in e.cases.iter() {
                        errors.push((case.name.to_utf8_string_lossy(), case.value));
                    }
                }
                ScSpecEntry::FunctionV0(f) => {
                    let inputs: Vec<(String, String)> = f
                        .inputs
                        .iter()
                        .map(|i| (i.name.to_utf8_string_lossy(), render_type(&i.type_)))
                        .collect();
                    let output = match f.outputs.iter().next() {
                        Some(def) => render_type(def),
                        None => "void".to_string(),
                    };
                    functions.push(format!(
                        "{{\n      \"name\": \"{}\",\n      \"inputs\": {},\n      \"output\": \"{}\"\n    }}",
                        f.name.0.to_utf8_string_lossy(),
                        render_name_type_list(&inputs, "      "),
                        output,
                    ));
                }
                other => std::panic!("unexpected spec entry: {other:?}"),
            }
        }

        format!(
            "{{\n  \"structs\": {},\n  \"enums\": {},\n  \"errors\": {},\n  \"functions\": {}\n}}\n",
            render_array(&structs, "  "),
            render_array(&enums, "  "),
            render_case_list(&errors, "  "),
            render_array(&functions, "  "),
        )
    }

    fn strip_whitespace(text: &str) -> String {
        text.chars().filter(|c| !c.is_whitespace()).collect()
    }

    #[test]
    fn spec_json_matches_contract_interface() {
        let expected = render_spec_json();
        if std::env::var("UPDATE_SPEC").is_ok() {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("spec.json");
            std::fs::write(&path, &expected).expect("failed to write spec.json");
            // SPEC_JSON was captured at compile time; skip the comparison on
            // the regeneration run and let the next plain run verify it.
            return;
        }
        // Whitespace-insensitive: no value in this document contains spaces,
        // so formatting cannot mask real drift and cannot cause false alarms.
        assert_eq!(
            strip_whitespace(SPEC_JSON),
            strip_whitespace(&expected),
            "spec.json is out of date with the contract interface.\n\
             Regenerate it with: UPDATE_SPEC=1 cargo test -p stealth-postage spec_json\n\
             Expected content:\n{expected}"
        );
    }

    #[test]
    fn spec_covers_every_public_contract_function() {
        // Every `pub fn` in this file lives in the #[contractimpl] block, so
        // scanning the source catches a new contract function that was not
        // added to the entries() list above (and therefore not to spec.json).
        let mut source_fns: Vec<&str> = LIB_RS
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim_start();
                let rest = trimmed.strip_prefix("pub fn ")?;
                Some(rest.split('(').next().unwrap_or(rest).trim())
            })
            .collect();
        source_fns.sort_unstable();
        source_fns.dedup();

        let mut spec_fns: Vec<String> = entries()
            .iter()
            .filter_map(|entry| match entry {
                ScSpecEntry::FunctionV0(f) => Some(f.name.0.to_utf8_string_lossy()),
                _ => None,
            })
            .collect();
        spec_fns.sort_unstable();

        assert_eq!(
            source_fns,
            spec_fns.iter().map(String::as_str).collect::<Vec<_>>(),
            "public contract functions and spec entries differ.\n\
             Add the missing spec_xdr_* entry to spec_check::entries() and \
             regenerate spec.json with: UPDATE_SPEC=1 cargo test -p stealth-postage spec_json"
        );
    }
}
