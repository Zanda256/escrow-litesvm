#[cfg(test)]
mod tests {
    use anchor_lang::{pubkey, require};
    use {
        anchor_lang::{
            prelude::msg, 
            solana_program::program_pack::Pack, 
            AccountDeserialize, 
            InstructionData, 
            ToAccountMetas
        }, anchor_spl::{
            associated_token::{
                self, 
                spl_associated_token_account
            }, 
            token::spl_token
        }, 
        litesvm::LiteSVM, 
        litesvm_token::{
            spl_token::ID as TOKEN_PROGRAM_ID, 
            CreateAssociatedTokenAccount, 
            CreateMint, MintTo
        }, 
        solana_rpc_client::rpc_client::RpcClient,
        solana_account::{Account, ReadableAccount},
        solana_instruction::Instruction, 
        solana_keypair::Keypair, 
        solana_message::Message, 
        solana_native_token::LAMPORTS_PER_SOL, 
        solana_pubkey::Pubkey, 
        solana_sdk_ids::system_program::ID as SYSTEM_PROGRAM_ID, 
        solana_signer::Signer, 
        solana_transaction::Transaction, 
        solana_address::Address, 
        std::{
            path::PathBuf, 
            str::FromStr
        }
    };
    use anchor_lang::solana_program::sysvar::clock::Clock;
    use crate::state::Escrow;

    static PROGRAM_ID: Pubkey = crate::ID;

    fn setup() -> (LiteSVM, Keypair) {
        // Initialize LiteSVM and payer
        let mut program = LiteSVM::new();
        let payer = Keypair::new();
    
        // Airdrop some SOL to the payer keypair
        program
            .airdrop(&payer.pubkey(), 50 * LAMPORTS_PER_SOL)
            .expect("Failed to airdrop SOL to payer");
    
        // Load program SO file
        let so_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/deploy/anchor_escrow.so");
    
        let program_data = std::fs::read(so_path).expect("Failed to read program SO file");
    
        let _ = program.add_program(PROGRAM_ID, &program_data);

        // Example on how to Load an account from devnet
        // let rpc_client = RpcClient::new("https://api.devnet.solana.com");
        // let account_address = Address::from_str("DRYvf71cbF2s5wgaJQvAGkghMkRcp5arvsK2w97vXhi2").unwrap();
        // let fetched_account = rpc_client
        //     .get_account(&account_address)
        //     .expect("Failed to fetch account from devnet");
        //
        // program.set_account(payer.pubkey(), Account {
        //     lamports: fetched_account.lamports,
        //     data: fetched_account.data,
        //     owner: Pubkey::from(fetched_account.owner.to_bytes()),
        //     executable: fetched_account.executable,
        //     rent_epoch: fetched_account.rent_epoch
        // }).unwrap();
        //
        // msg!("Lamports of fetched account: {}", fetched_account.lamports);
    
        // Return the LiteSVM instance and payer keypair
        (program, payer)
    }

    #[test]
    fn test_make() {

        // Setup the test environment by initializing LiteSVM and creating a payer keypair
        let (mut program, payer) = setup();

        // Get the maker's public key from the payer keypair
        let maker = payer.pubkey();
        
        // Create two mints (Mint A and Mint B) with 6 decimal places and the maker as the authority
        let mint_a = CreateMint::new(&mut program, &payer)
            .decimals(6)
            .authority(&maker)
            .send()
            .unwrap();
        msg!("Mint A: {}\n", mint_a);

        let mint_b = CreateMint::new(&mut program, &payer)
            .decimals(6)
            .authority(&maker)
            .send()
            .unwrap();
        msg!("Mint B: {}\n", mint_b);

        // Create the maker's associated token account for Mint A
        let maker_ata_a = CreateAssociatedTokenAccount::new(&mut program, &payer, &mint_a)
            .owner(&maker).send().unwrap();
        msg!("Maker ATA A: {}\n", maker_ata_a);

        // Derive the PDA for the escrow account using the maker's public key and a seed value
        let escrow = Pubkey::find_program_address(
            &[b"escrow", maker.as_ref(), &123u64.to_le_bytes()],
            &PROGRAM_ID
        ).0;
        msg!("Escrow PDA: {}\n", escrow);

        // Derive the PDA for the vault associated token account using the escrow PDA and Mint A
        let vault = associated_token::get_associated_token_address(&escrow, &mint_a);
        msg!("Vault PDA: {}\n", vault);

        // Define program IDs for associated token program, token program, and system program
        let asspciated_token_program = spl_associated_token_account::ID;
        let token_program = TOKEN_PROGRAM_ID;
        let system_program = SYSTEM_PROGRAM_ID;

        // Mint 1,000 tokens (with 6 decimal places) of Mint A to the maker's associated token account
        MintTo::new(&mut program, &payer, &mint_a, &maker_ata_a, 1000000000)
            .send()
            .unwrap();

        // Create the "Make" instruction to deposit tokens into the escrow
        let make_ix = Instruction {
            program_id: PROGRAM_ID,
            accounts: crate::accounts::Make {
                maker: maker,
                mint_a: mint_a,
                mint_b: mint_b,
                maker_ata_a: maker_ata_a,
                escrow: escrow,
                vault: vault,
                associated_token_program: asspciated_token_program,
                token_program: token_program,
                system_program: system_program,
            }.to_account_metas(None),
            data: crate::instruction::Make {deposit: 10, seed: 123u64, receive: 10 , lock_period: 10}.data(),
        };

        // Create and send the transaction containing the "Make" instruction
        let message = Message::new(&[make_ix], Some(&payer.pubkey()));
        let recent_blockhash = program.latest_blockhash();

        let transaction = Transaction::new(&[&payer], message, recent_blockhash);

        // Send the transaction and capture the result
        let tx = program.send_transaction(transaction).unwrap();

        // Log transaction details
        msg!("\n\nMake transaction sucessfull");
        msg!("CUs Consumed: {}", tx.compute_units_consumed);
        msg!("Tx Signature: {}", tx.signature);

        // Verify the vault account and escrow account data after the "Make" instruction
        let vault_account = program.get_account(&vault).unwrap();
        let vault_data = spl_token::state::Account::unpack(&vault_account.data).unwrap();
        assert_eq!(vault_data.amount, 10);
        assert_eq!(vault_data.owner, escrow);
        assert_eq!(vault_data.mint, mint_a);

        let escrow_account = program.get_account(&escrow).unwrap();
        let escrow_data = crate::state::Escrow::try_deserialize(&mut escrow_account.data.as_ref()).unwrap();
        assert_eq!(escrow_data.seed, 123u64);
        assert_eq!(escrow_data.maker, maker);
        assert_eq!(escrow_data.mint_a, mint_a);
        assert_eq!(escrow_data.mint_b, mint_b);
        assert_eq!(escrow_data.receive, 10);
        assert_eq!(escrow_data.lock_period, 10);
        msg!("escrow_data.start_time: {}\n", escrow_data.start_time);
        
    }

    #[test]
    fn test_refund() {

        // Setup the test environment by initializing LiteSVM and creating a payer keypair
        let (mut program, payer) = crate::tests::tests::setup();

        // Get the maker's public key from the payer keypair
        let maker = payer.pubkey();

        // Create two mints (Mint A and Mint B) with 6 decimal places and the maker as the authority
        let mint_a = CreateMint::new(&mut program, &payer)
            .decimals(6)
            .authority(&maker)
            .send()
            .unwrap();
        msg!("Mint A: {}\n", mint_a);

        let mint_b = CreateMint::new(&mut program, &payer)
            .decimals(6)
            .authority(&maker)
            .send()
            .unwrap();
        msg!("Mint B: {}\n", mint_b);

        // Create the maker's associated token account for Mint A
        let maker_ata_a = CreateAssociatedTokenAccount::new(&mut program, &payer, &mint_a)
            .owner(&maker).send().unwrap();
        msg!("Maker ATA A: {}\n", maker_ata_a);

        // Derive the PDA for the escrow account using the maker's public key and a seed value
        let escrow = Pubkey::find_program_address(
            &[b"escrow", maker.as_ref(), &123u64.to_le_bytes()],
            &crate::tests::tests::PROGRAM_ID
        ).0;
        msg!("Escrow PDA: {}\n", escrow);

        // Derive the PDA for the vault associated token account using the escrow PDA and Mint A
        let vault = associated_token::get_associated_token_address(&escrow, &mint_a);
        msg!("Vault PDA: {}\n", vault);

        // Define program IDs for associated token program, token program, and system program
        let asspciated_token_program = spl_associated_token_account::ID;
        let token_program = TOKEN_PROGRAM_ID;
        let system_program = SYSTEM_PROGRAM_ID;

        // Mint 1,000 tokens (with 6 decimal places) of Mint A to the maker's associated token account
        MintTo::new(&mut program, &payer, &mint_a, &maker_ata_a, 1000000000)
            .send()
            .unwrap();

        // Create the "Make" instruction to deposit tokens into the escrow
        let make_ix = Instruction {
            program_id: crate::tests::tests::PROGRAM_ID,
            accounts: crate::accounts::Make {
                maker: maker,
                mint_a: mint_a,
                mint_b: mint_b,
                maker_ata_a: maker_ata_a,
                escrow: escrow,
                vault: vault,
                associated_token_program: asspciated_token_program,
                token_program: token_program,
                system_program: system_program,
            }.to_account_metas(None),
            data: crate::instruction::Make { deposit: 10, seed: 123u64, receive: 10,lock_period: 10}.data(),
        };

        // Create and send the transaction containing the "Make" instruction
        let message = Message::new(&[make_ix], Some(&payer.pubkey()));
        let recent_blockhash = program.latest_blockhash();

        let transaction = Transaction::new(&[&payer], message, recent_blockhash);

        // Send the transaction and capture the result
        let tx = program.send_transaction(transaction).unwrap();

        // Log transaction details
        msg!("\n\nTestRefund : Make transaction successful");
        msg!("CUs Consumed: {}", tx.compute_units_consumed);
        msg!("Tx Signature: {}", tx.signature);


        // Create the "Refund" instruction to deposit tokens into the escrow
        let make_ix = Instruction {
            program_id: crate::tests::tests::PROGRAM_ID,
            accounts: crate::accounts::Refund {
                maker: maker,
                mint_a: mint_a,
                maker_ata_a: maker_ata_a,
                escrow: escrow,
                vault: vault,
                token_program: token_program,
                system_program: system_program,
            }.to_account_metas(None),
            data: crate::instruction::Refund {}.data(),
        };

        // Create and send the transaction containing the "Refund" instruction
        let message = Message::new(&[make_ix], Some(&payer.pubkey()));
        let recent_blockhash = program.latest_blockhash();

        let transaction = Transaction::new(&[&payer], message, recent_blockhash);

        // Send the transaction and capture the result
        let tx = program.send_transaction(transaction).unwrap();
        // Log transaction details
        msg!("\n\nTestRefund : Refund transaction successful");
        msg!("CUs Consumed: {}", tx.compute_units_consumed);
        msg!("Tx Signature: {}", tx.signature);

        let vault_acc_result = program.get_account(&vault);
        assert!(vault_acc_result.is_none(), "Expected vault Account not to exist after refund");
    }

    #[test]
    fn test_take() {

        // Setup the test environment by initializing LiteSVM and creating a payer keypair
        let (mut program, payer) = setup();

        // Get the taker's public key from the payer keypair
        let taker = Keypair::new();

        // Airdrop some SOL to the payer keypair
        program
            .airdrop(&taker.pubkey(), 50 * LAMPORTS_PER_SOL)
            .expect("Failed to airdrop SOL to payer");

        // Get the maker's public key from the payer keypair
        let maker = payer.pubkey();

        // Create two mints (Mint A and Mint B) with 6 decimal places and the maker as the authority
        let mint_a = CreateMint::new(&mut program, &payer)
            .decimals(6)
            .authority(&maker)
            .send()
            .unwrap();
        msg!("test_take: Mint A: {}\n", mint_a);

        let mint_b = CreateMint::new(&mut program, &payer)
            .decimals(6)
            .authority(&maker)
            .send()
            .unwrap();
        msg!("test_take: Mint B: {}\n", mint_b);

        // Create the maker's associated token account for Mint A
        let maker_ata_a = CreateAssociatedTokenAccount::new(&mut program, &payer, &mint_a)
            .owner(&maker).send().unwrap();
        msg!("test_take: Maker ATA A: {}\n", maker_ata_a);

        // Create the maker's associated token account for Mint B
        let maker_ata_b = CreateAssociatedTokenAccount::new(&mut program, &payer, &mint_b)
            .owner(&maker).send().unwrap();
        msg!("test_take: Maker ATA B: {}\n", maker_ata_b);

        // Create the taker's associated token account for Mint A
        let taker_ata_a = CreateAssociatedTokenAccount::new(&mut program, &payer, &mint_a)
            .owner(&taker.pubkey()).send().unwrap();
        msg!("test_take: Taker ATA A: {}\n", taker_ata_a);

        // Create the taker's associated token account for Mint B
        let taker_ata_b = CreateAssociatedTokenAccount::new(&mut program, &payer, &mint_b)
            .owner(&taker.pubkey()).send().unwrap();
        msg!("test_take: Taker ATA A: {}\n", taker_ata_b);

        // Derive the PDA for the escrow account using the maker's public key and a seed value
        let escrow_seeds = &[b"escrow", maker.as_ref(), &123u64.to_le_bytes()];
        let escrow = Pubkey::find_program_address(
            &[b"escrow", maker.as_ref(), &123u64.to_le_bytes()],
            &crate::tests::tests::PROGRAM_ID
        ).0;
        msg!("test_take: Escrow PDA: {}\n", escrow);

        // Derive the PDA for the vault associated token account using the escrow PDA and Mint A
        let vault = associated_token::get_associated_token_address(&escrow, &mint_a);
        msg!("test_take: Vault PDA: {}\n", vault);

        // Define program IDs for associated token program, token program, and system program
        let associated_token_program = spl_associated_token_account::ID;
        let token_program = TOKEN_PROGRAM_ID;
        let system_program = SYSTEM_PROGRAM_ID;

        // Mint 1,000 tokens (with 6 decimal places) of Mint A to the maker's associated token account
        //
        MintTo::new(&mut program, &payer, &mint_b, &taker_ata_b, 1000000000)
            .send()
            .unwrap();

        MintTo::new(&mut program, &payer, &mint_a, &maker_ata_a, 1000000000)
            .send()
            .unwrap();

        // Create the "Make" instruction to deposit tokens into the escrow
        let make_ix = Instruction {
            program_id: crate::tests::tests::PROGRAM_ID,
            accounts: crate::accounts::Make {
                maker: maker,
                mint_a: mint_a,
                mint_b: mint_b,
                maker_ata_a: maker_ata_a,
                escrow: escrow,
                vault: vault,
                associated_token_program: associated_token_program,
                token_program: token_program,
                system_program: system_program,
            }.to_account_metas(None),
            data: crate::instruction::Make { deposit: 10, seed: 123u64, receive: 10, lock_period:10 }.data(),
        };

        // Create and send the transaction containing the "Make" instruction
        let message = Message::new(&[make_ix], Some(&payer.pubkey()));
        let recent_blockhash = program.latest_blockhash();

        let transaction = Transaction::new(&[&payer], message, recent_blockhash);

        // Send the transaction and capture the result
        let tx = program.send_transaction(transaction).unwrap();

        // Log transaction details
        msg!("\n\ntest_take: Make transaction sucessfull");
        msg!("CUs Consumed: {}", tx.compute_units_consumed);
        msg!("Tx Signature: {}", tx.signature);

// --------------------------------------------------------------------------------------------------------------------------------
        // program.expire_blockhash();
        // Create the "take" instruction to test that escrow is unlocked after lock-period time elapses


        // let e:Account = program.get_account(&escrow).unwrap();
        // let mut data_slice: &[u8] = &e.data;
        // match Escrow::try_deserialize(&mut data_slice) {
        //     Ok(data) => {
        //         msg!("Successfully deserialized vault:");
        //         let unlock_slot = data.start_time.checked_add(data.lock_period).unwrap();
        //         msg!("unlock_slot: {}", unlock_slot);
        //     }
        //     Err(e) => {
        //         panic!("{}",e)
        //     }
        // }

        // current_clock.unix_timestamp = 1735689600;
        // &program.set_sysvar::<Clock>(&current_clock);
        // program.warp_to_slot(current_clock.slot.checked_add(100).unwrap());
        program.warp_to_slot(10);
        let current_clock = program.get_sysvar::<Clock>();
        msg!("Current slot from tests after warp: {:?}", current_clock);

        let take_ix1 = Instruction {
            program_id: crate::tests::tests::PROGRAM_ID,
            accounts: crate::accounts::Take {
                taker:taker.pubkey(),
                maker: maker,
                mint_a: mint_a,
                mint_b:mint_b,
                taker_ata_a: taker_ata_a,
                taker_ata_b: taker_ata_b,
                maker_ata_b: maker_ata_b,
                escrow: escrow,
                vault: vault,
                associated_token_program: associated_token_program,
                token_program: token_program,
                system_program: system_program,
            }.to_account_metas(None),
            data: crate::instruction::Take {}.data(),
        };

        // Create and send the transaction containing the "Refund" instruction
        let message1 = Message::new(&[take_ix1], Some(&taker.pubkey()));
        let recent_blockhash1 = program.latest_blockhash();

        // [b"escrow", maker.key().as_ref(), seed.to_le_bytes().as_ref()],

        let transaction1 = Transaction::new(&[&taker], message1, recent_blockhash1);

        // Send the transaction and capture the result
        let res1 = program.send_transaction(transaction1);
        let mut ok = false;
        match res1 {
            Ok(tx) => {
                // Log transaction details
                msg!("\n\ntest_take transaction successful");
                msg!("CUs Consumed: {}", tx.compute_units_consumed);
                msg!("Tx Signature: {}", tx.signature);
                msg!("Tx Logs: {:?}", tx.logs);
                ok = true;
            },

            Err(err) => {
                msg!("\n\ntest_take transaction failed with {:?}", err);
            }
        }

        assert!(ok, "Expected take to pass after lock period elapses");

        let vault_acc_result = program.get_account(&vault);
        assert!(vault_acc_result.is_none(), "Expected vault Account not to exist after refund");

        // Verify token transfers
        let taker_ata_a_account = program.get_account(&taker_ata_a).unwrap();
        let taker_ata_a_data = spl_token::state::Account::unpack(&taker_ata_a_account.data).unwrap();
        assert_eq!(taker_ata_a_data.amount, 10, "Expected Taker Account to have 10 tokens");
    }

    #[test]
    fn test_take_before_lock_period_elapses() {

        // Setup the test environment by initializing LiteSVM and creating a payer keypair
        let (mut program, payer) = crate::tests::tests::setup();

        // Get the taker's public key from the payer keypair
        let taker = Keypair::new();

        // Airdrop some SOL to the payer keypair
        program
            .airdrop(&taker.pubkey(), 50 * LAMPORTS_PER_SOL)
            .expect("Failed to airdrop SOL to payer");

        // Get the maker's public key from the payer keypair
        let maker = payer.pubkey();

        // Create two mints (Mint A and Mint B) with 6 decimal places and the maker as the authority
        let mint_a = CreateMint::new(&mut program, &payer)
            .decimals(6)
            .authority(&maker)
            .send()
            .unwrap();
        msg!("test_take: Mint A: {}\n", mint_a);

        let mint_b = CreateMint::new(&mut program, &payer)
            .decimals(6)
            .authority(&maker)
            .send()
            .unwrap();
        msg!("test_take: Mint B: {}\n", mint_b);

        // Create the maker's associated token account for Mint A
        let maker_ata_a = CreateAssociatedTokenAccount::new(&mut program, &payer, &mint_a)
            .owner(&maker).send().unwrap();
        msg!("test_take: Maker ATA A: {}\n", maker_ata_a);

        // Create the maker's associated token account for Mint B
        let maker_ata_b = CreateAssociatedTokenAccount::new(&mut program, &payer, &mint_b)
            .owner(&maker).send().unwrap();
        msg!("test_take: Maker ATA B: {}\n", maker_ata_b);

        // Create the taker's associated token account for Mint A
        let taker_ata_a = CreateAssociatedTokenAccount::new(&mut program, &payer, &mint_a)
            .owner(&taker.pubkey()).send().unwrap();
        msg!("test_take: Taker ATA A: {}\n", taker_ata_a);

        // Create the taker's associated token account for Mint B
        let taker_ata_b = CreateAssociatedTokenAccount::new(&mut program, &payer, &mint_b)
            .owner(&taker.pubkey()).send().unwrap();
        msg!("test_take: Taker ATA A: {}\n", taker_ata_b);

        // Derive the PDA for the escrow account using the maker's public key and a seed value
        let escrow_seeds = &[b"escrow", maker.as_ref(), &123u64.to_le_bytes()];
        let escrow = Pubkey::find_program_address(
            &[b"escrow", maker.as_ref(), &123u64.to_le_bytes()],
            &crate::tests::tests::PROGRAM_ID
        ).0;
        msg!("test_take: Escrow PDA: {}\n", escrow);

        // Derive the PDA for the vault associated token account using the escrow PDA and Mint A
        let vault = associated_token::get_associated_token_address(&escrow, &mint_a);
        msg!("test_take: Vault PDA: {}\n", vault);

        // Define program IDs for associated token program, token program, and system program
        let associated_token_program = spl_associated_token_account::ID;
        let token_program = TOKEN_PROGRAM_ID;
        let system_program = SYSTEM_PROGRAM_ID;

        // Mint 1,000 tokens (with 6 decimal places) of Mint A to the maker's associated token account
        //
        MintTo::new(&mut program, &payer, &mint_b, &taker_ata_b, 1000000000)
            .send()
            .unwrap();

        MintTo::new(&mut program, &payer, &mint_a, &maker_ata_a, 1000000000)
            .send()
            .unwrap();

        // Create the "Make" instruction to deposit tokens into the escrow
        let make_ix = Instruction {
            program_id: crate::tests::tests::PROGRAM_ID,
            accounts: crate::accounts::Make {
                maker: maker,
                mint_a: mint_a,
                mint_b: mint_b,
                maker_ata_a: maker_ata_a,
                escrow: escrow,
                vault: vault,
                associated_token_program: associated_token_program,
                token_program: token_program,
                system_program: system_program,
            }.to_account_metas(None),
            data: crate::instruction::Make { deposit: 10, seed: 123u64, receive: 10, lock_period:10 }.data(),
        };

        // Create and send the transaction containing the "Make" instruction
        let message = Message::new(&[make_ix], Some(&payer.pubkey()));
        let recent_blockhash = program.latest_blockhash();

        let transaction = Transaction::new(&[&payer], message, recent_blockhash);

        // Send the transaction and capture the result
        let tx = program.send_transaction(transaction).unwrap();

        // Log transaction details
        msg!("\n\ntest_take: Make transaction sucessfull");
        msg!("CUs Consumed: {}", tx.compute_units_consumed);
        msg!("Tx Signature: {}", tx.signature);

        // let current_slot = program.get_sysvar::<Clock>().slot;
        // msg!("Current slot: {}", current_slot);
        // program.warp_to_slot(current_slot + 100);

        // Create the "take" instruction to test that escrow is locked before lock-period time elapses
         let take_ix = Instruction {
             program_id: crate::tests::tests::PROGRAM_ID,
             accounts: crate::accounts::Take {
                 taker:taker.pubkey(),
                 maker: maker,
                 mint_a: mint_a,
                 mint_b:mint_b,
                 taker_ata_a: taker_ata_a,
                 taker_ata_b: taker_ata_b,
                 maker_ata_b: maker_ata_b,
                 escrow: escrow,
                 vault: vault,
                 associated_token_program: associated_token_program,
                 token_program: token_program,
                 system_program: system_program,
             }.to_account_metas(None),
             data: crate::instruction::Take {}.data(),
         };

         // Create and send the transaction containing the "Refund" instruction
         let message = Message::new(&[take_ix], Some(&taker.pubkey()));
         let recent_blockhash = program.latest_blockhash();

        // [b"escrow", maker.key().as_ref(), seed.to_le_bytes().as_ref()],

         let transaction = Transaction::new(&[&taker], message, recent_blockhash);

         // Send the transaction and capture the result
         let res = program.send_transaction(transaction);

         assert!(res.is_err(), "Expected take to fail before lock period elapses");
         if let  Err(tx) = res {
            msg!("\n\ntest: take_before_lock_period_elapses successful");
            msg!("Test details: {:?}", tx);;
         };

        // Verify tokens in vault
        let vault_acc = program.get_account(&vault).unwrap();
        let vault_acc_data = spl_token::state::Account::unpack(&vault_acc.data).unwrap();
        assert_eq!(vault_acc_data.amount, 10, "Expected vault Account to have 10 tokens");

        // --------------------------------------------------------------------------------------------------------------------------------

    }
}