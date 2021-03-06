// Copyright 2017 The Exonum Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

extern crate cryptocurrency;
extern crate exonum;
extern crate exonum_testkit;

use exonum::crypto::{self, PublicKey, SecretKey};
use exonum::messages::Message;
use exonum_testkit::{ApiKind, TestKit, TestKitApi, TestKitBuilder};

// Import datatypes used in tests from the crate where the service is defined.
use cryptocurrency::{TxCreateWallet, TxTransfer, TransactionResponse, Wallet, CurrencyService};

/// Wrapper for the cryptocurrency service API allowing to easily use it
/// (compared to `TestKitApi` calls).
struct CryptocurrencyApi {
    inner: TestKitApi,
}

impl CryptocurrencyApi {
    /// Generates a wallet creation transaction with a random key pair, sends it over HTTP,
    /// and checks the synchronous result (i.e., the hash of the transaction returned
    /// within the `TransactionResponse` struct).
    /// Note that the transaction is not immediately added to the blockchain, but rather is put
    /// to the pool of unconfirmed transactions.
    fn create_wallet(&self, name: &str) -> (TxCreateWallet, SecretKey) {
        let (pubkey, key) = crypto::gen_keypair();
        // Create a presigned transaction
        let tx = TxCreateWallet::new(&pubkey, name, &key);

        let tx_info: TransactionResponse = self.inner.post(
            ApiKind::Service("cryptocurrency"),
            "v1/wallets",
            &tx,
        );
        assert_eq!(tx_info.tx_hash, tx.hash());
        (tx, key)
    }

    /// Sends a transfer transaction over HTTP and checks the synchronous result.
    fn transfer(&self, tx: &TxTransfer) {
        let tx_info: TransactionResponse = self.inner.post(
            ApiKind::Service("cryptocurrency"),
            "v1/wallets/transfer",
            tx,
        );
        assert_eq!(tx_info.tx_hash, tx.hash());
    }

    /// Gets the state of a particular wallet using an HTTP request.
    fn get_wallet(&self, pubkey: &PublicKey) -> Wallet {
        self.inner.get(
            ApiKind::Service("cryptocurrency"),
            &format!("v1/wallet/{}", pubkey.to_string()),
        )
    }

    /// Asserts that a wallet with the specified public key is not known to the blockchain.
    fn assert_no_wallet(&self, pubkey: &PublicKey) {
        let err: String = self.inner.get_err(
            ApiKind::Service("cryptocurrency"),
            &format!("v1/wallet/{}", pubkey.to_string()),
        );
        assert_eq!(err, "Wallet not found".to_string());
    }
}

/// Creates a testkit together with the API wrapper defined above.
fn create_testkit() -> (TestKit, CryptocurrencyApi) {
    let testkit = TestKitBuilder::validator()
        .with_service(CurrencyService)
        .create();
    let api = CryptocurrencyApi { inner: testkit.api() };
    (testkit, api)
}

/// Check that the wallet creation transaction works when invoked via API.
#[test]
fn test_create_wallet() {
    let (mut testkit, api) = create_testkit();
    // Create and send a transaction via API
    let (tx, _) = api.create_wallet("Alice");
    testkit.create_block();

    // Check that the user indeed is persisted by the service
    let wallet = api.get_wallet(tx.pub_key());
    assert_eq!(wallet.pub_key(), tx.pub_key());
    assert_eq!(wallet.name(), tx.name());
    assert_eq!(wallet.balance(), 100);
}

/// Check that the transfer transaction works as intended.
#[test]
fn test_transfer() {
    // Create 2 wallets.
    let (mut testkit, api) = create_testkit();
    let (tx_alice, key_alice) = api.create_wallet("Alice");
    let (tx_bob, _) = api.create_wallet("Bob");
    testkit.create_block();

    // Check that the initial Alice's and Bob's balances persisted by the service.
    let wallet = api.get_wallet(tx_alice.pub_key());
    assert_eq!(wallet.balance(), 100);
    let wallet = api.get_wallet(tx_bob.pub_key());
    assert_eq!(wallet.balance(), 100);

    // Transfer funds by invoking the corresponding API method.
    let tx = TxTransfer::new(
        tx_alice.pub_key(),
        tx_bob.pub_key(),
        10, // transferred amount
        0, // seed
        &key_alice,
    );
    api.transfer(&tx);
    testkit.create_block();

    // After the transfer transaction is included into a block, we may check new wallet
    // balances.
    let wallet = api.get_wallet(tx_alice.pub_key());
    assert_eq!(wallet.balance(), 90);
    let wallet = api.get_wallet(tx_bob.pub_key());
    assert_eq!(wallet.balance(), 110);
}

/// Check that a transfer from a non-existing wallet fails as expected.
#[test]
fn test_transfer_from_nonexisting_wallet() {
    let (mut testkit, api) = create_testkit();

    let (tx_alice, key_alice) = api.create_wallet("Alice");
    let (tx_bob, _) = api.create_wallet("Bob");
    // Do not commit Alice's transaction, so Alice's wallet does not exist
    // when a transfer occurs.
    testkit.create_block_with_tx_hashes(&[tx_bob.hash()]);

    api.assert_no_wallet(tx_alice.pub_key());
    let wallet = api.get_wallet(tx_bob.pub_key());
    assert_eq!(wallet.balance(), 100);

    let tx = TxTransfer::new(
        tx_alice.pub_key(),
        tx_bob.pub_key(),
        10, // transfer amount
        0, // seed
        &key_alice,
    );
    api.transfer(&tx);
    testkit.create_block_with_tx_hashes(&[tx.hash()]);

    // Check that Bob's balance doesn't change
    let wallet = api.get_wallet(tx_bob.pub_key());
    assert_eq!(wallet.balance(), 100);
}

/// Check that a transfer to a non-existing wallet fails as expected.
#[test]
fn test_transfer_to_nonexisting_wallet() {
    let (mut testkit, api) = create_testkit();

    let (tx_alice, key_alice) = api.create_wallet("Alice");
    let (tx_bob, _) = api.create_wallet("Bob");
    // Do not commit Bob's transaction, so Bob's wallet does not exist
    // when a transfer occurs.
    testkit.create_block_with_tx_hashes(&[tx_alice.hash()]);

    let wallet = api.get_wallet(tx_alice.pub_key());
    assert_eq!(wallet.balance(), 100);
    api.assert_no_wallet(tx_bob.pub_key());

    let tx = TxTransfer::new(
        tx_alice.pub_key(),
        tx_bob.pub_key(),
        10, // transfer amount
        0, // seed
        &key_alice,
    );
    api.transfer(&tx);
    testkit.create_block_with_tx_hashes(&[tx.hash()]);

    // Check that Alice's balance doesn't change
    let wallet = api.get_wallet(tx_alice.pub_key());
    assert_eq!(wallet.balance(), 100);
}

/// Check that an overcharge does not lead to changes in sender's and receiver's balances.
#[test]
fn test_transfer_overcharge() {
    let (mut testkit, api) = create_testkit();

    let (tx_alice, key_alice) = api.create_wallet("Alice");
    let (tx_bob, _) = api.create_wallet("Bob");
    testkit.create_block();

    // Transfer funds. The transfer amount (110) is more than Alice has (100).
    let tx = TxTransfer::new(
        tx_alice.pub_key(),
        tx_bob.pub_key(),
        110, // transfer amount
        0, // seed
        &key_alice,
    );
    api.transfer(&tx);
    testkit.create_block();

    let wallet = api.get_wallet(tx_alice.pub_key());
    assert_eq!(wallet.balance(), 100);
    let wallet = api.get_wallet(tx_bob.pub_key());
    assert_eq!(wallet.balance(), 100);
}
