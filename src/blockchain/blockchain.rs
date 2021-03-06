//! # Interact with blockchain
//!
//! Post/read information to/from blockchain
//! Information posted is a merkle root

use crate::blockchain::merkle::{CryptoSHA3256Hash, new_tree, CryptoHashData, store_tree};
use crate::Result;
use crate::voter_roster::VoterRoster;
use crate::poll_configuration::PollConfiguration;
use crate::planes::Plane;
use crate::debug;

use web3::types::{BlockNumber, Address, TransactionParameters, U256, CallRequest};
use web3::signing::Key;
use hex;
use secp256k1::SecretKey;
use web3::signing::SecretKeyRef;
use std::fs::File;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NetworkConfig {
    node: String,
    key: String,
}

// returns block #
pub fn retrieve_from_chain(value: Vec<u8>) -> u64 {
    let _value = value;
    0
}

// Load blockchain network configurations
fn load_xxn() -> Result<NetworkConfig>{
    let config = "examples/xxn_config.yaml";
    let config = File::open(config)?;
    let config: NetworkConfig  = serde_yaml::from_reader(config).expect("Error loading XXN config file");

    Ok(config)
}

pub fn post(data: CryptoSHA3256Hash) -> Result<()> {
    // Load configuration file
    let config = load_xxn()?;

    // Get private key from config
    let key = SecretKey::from_slice(&hex::decode(config.key)?)?;
    let key = SecretKeyRef::new(&key);
    
    // Get public address of private key
    let pub_addr: Address = key.address();
    let uri = config.node;

    // Placeholder request to be used to estimate gas
    let req = CallRequest {
        from: None,
        to: None,
        gas: None,
        gas_price: None,
        value: None,
        data: None
    };

    // Start web3 class
    let transport = web3::transports::Http::new(&uri).unwrap();
    let web3 = web3::Web3::new(transport);
    
    let send = async {
        // Get last block and estimate gas
        let block_number = web3.eth().block_number().await.expect("Error getting last block number");
        let gas = web3.eth().estimate_gas(req, Some(BlockNumber::Number(block_number))).await.expect("Error getting gas value");

        // Build transaction with data to post
        let params = TransactionParameters {
            nonce: None,
            to: Some(pub_addr), // Send to own address
            gas_price: None,
            chain_id: None,
            data: data.into(), // Data to be posted
            value: U256::zero(),
            gas: gas
        };

        // Sign transaction before posting
        let signed = web3.accounts().sign_transaction(params, key).await.expect("Error signing transaction");
        let transaction = signed.raw_transaction;

        // Send signed transaction
        let sent = web3.eth().send_raw_transaction(transaction.into()).await.expect("Error sending transaction");
        debug!("Transaction Hash: {:?}", sent);

    };

    web3::block_on(send);
    Ok(())   
}

pub fn commit (pollconf: PollConfiguration, planes: Vec<Plane>) -> Result<()> {
    // Re-construct roster
    let roster: VoterRoster = {
        let encoded_roster = pollconf.voter_roster.clone().unwrap();
        let decoded_roster = base64::decode(&encoded_roster.0).unwrap();
        let serialized_roster = std::str::from_utf8(&decoded_roster).unwrap();
        serde_yaml::from_str(serialized_roster).unwrap()
    };

    // Get voter info
    let roster = roster.records.into_iter()
        .map(|voter| {
            let ser_v = serde_yaml::to_string(&voter).unwrap();
            ser_v
        }).collect();


    // Re-construct the audited ballots.
    let audited_ballots = pollconf.audited_ballots.to_owned().unwrap();
    
    // Start vec of data for the tree
    // Push roster
    let mut data = CryptoHashData::new(roster);

    // Push audited ballots
    data.push_vec(audited_ballots);
   
    // Push planes
    planes.into_iter().for_each(|plane|
    {        
        plane.rows.into_iter().for_each(|row|
        {
            let ser_row = row.serializable(pollconf.num_ballots);

            // Each row cell is a leaf
            data.push(ser_row.col1);
            data.push(ser_row.col3);
        });
    });

    // After all data is in vec, pad it to be pow 2
    data.pad();


    // Create new tree with Vec of data
    let merkle_tree = new_tree(data).unwrap();
    debug!("Root: {}", hex::encode(merkle_tree.root()));

    // Store full tree in file, to be later used for proof of inclusions
    store_tree(&merkle_tree, String::from("merkle.yaml"))?;

    // Post root to blockchain
    post(merkle_tree.root())
}