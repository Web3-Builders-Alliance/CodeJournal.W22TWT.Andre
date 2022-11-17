
// THIS FILE CONTAINS A SET OF UTILITY FUNCTIONS, THAT BECAUSE OF THEIR USAGE IN MULTIPLE FILES,
// ARE CONDENSED HERE, WITH THE PORPUSE OF REUSIBILITY

use std::time::{
    SystemTime,   
    UNIX_EPOCH
  };
  
  use cosmwasm_std::{
    DepsMut,
    MessageInfo,
    Coin,
    Uint128,
    Storage,
    Addr,
    Timestamp
  };
  
  use cw721_base::{ MintMsg };
  use cw721_base::state::{ TokenInfo };
  
  // use crate::msg::StoreConf;
  
  use crate::state::{
    CW721Contract,
    Extension,
    CONFIG,
    Config,
    BURNT_AMOUNT,
    BURNT_LIST,
    BURNED
  };
  
  use crate::error::ContractError;
  
  
  // USING THE 'Timestamp' STRUCT AND THE RESPECTIVE ASSOCIATED FUNCTION 'from_seconds' FROM THE 'cosmwasm_std' CRATE, 
  // A TIMESTAMP IS CREATED REPRESENTING THE TIME PASSED SINCE EPOCH AT THE PRECISE MOMENT IT IS CALLLED.
  // 'from_seconds' TAKES A 'u64' AS A PARAMETER, THAT SHOULD REPRESENT THE NUMBER OF SECONDS PASSED SINCE EPOCH.
  
  pub fn _now() -> Timestamp {
    Timestamp::from_seconds(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs())
  }
  

  // FUNCTION THAT EVALUATES IF THE CALLER OF THE CONTRACT HAVE THE SAME ADDRESS OF THE MINTER INFORMATION STORED IN THE CONTRACT
  pub fn can_update(
    deps: &DepsMut,
    info: &MessageInfo // 
  ) -> Result<(), ContractError> {
    let cw721_contract = CW721Contract::default();
  
    let minter = cw721_contract.minter.load(deps.storage)?; // Load minter address from storage
  
    if info.sender != minter {
        return Err(ContractError::Unauthorized {});
    }

   
   // ANOTHER WAY TO IMPLEMENT THE CODE ABOVE //
   // match w721_contract.minter.load(deps.storage) {
   //     Ok(m) => {
   //       if info.sender != minter {
   //       return Err(ContractError::Unauthorized {});
   //       }
   //     }
   //     Err => Err(ContractError::MinterNotSetOrImpossibleToRetrieve {})
   // } 
 
    Ok(())
  }
  

  pub fn update_burnt_amount(
    storage: &mut dyn Storage, // mutable storage
    sender: &Addr,
  ) -> Result<(), ContractError> {
    match BURNT_AMOUNT.load(storage, sender) { // trying to load the value associated with the key, that in this case is the 'sender'
      Ok(mut amount) => {                      // Using match to unwrap the result. mut keyword added to allow the mutability of the value
          amount += Uint128::from(1u32);       // As stated in the docs, Uint128 is: A thin wrapper around u128 that is using strings for JSON encoding/decoding,
          BURNT_AMOUNT.save(storage, sender, &amount)?; // saving the modified value into the Map. This action will override the previous value associated with the key
          Ok(())
      },
      Err(_) => {
          BURNT_AMOUNT.save(storage, sender, &Uint128::from(1u32))?; // if the load fails, the code will reach this point. The programmer is assuming that the only way to get
          Ok(())                                                     // an error is in the case of the provided key don't doesn't exist in the Map. This is not true, because you could get here in
                                                                     // if an parse error happens. 
      }
    }
  }
  
  pub fn update_burnt_list(
    storage: &mut dyn Storage,
    sender: &Addr,
    token: &str,
  ) -> Result<(), ContractError> {
    // The BURNT_LIST stores the information of which addresses burnt what
    match BURNT_LIST.load(storage, sender) {
      Ok(mut list) => { // unwraps the value as mutable
          list.push(String::from(token)); // adds the token id to the list, that in this case is a vec of strings. 
          BURNT_LIST.save(storage, sender, &list)?; // saves the list after the mutable vec is updated with the new value
          Ok(())
      },
      Err(_) => {
          BURNT_LIST.save(storage, sender, &vec![String::from(token)])?; // the same as the function before this one
          Ok(())
      }
    }
  }
  
  pub fn burn_token(
    contract: &CW721Contract,
    storage: &mut dyn Storage,
    token_id: String
  ) -> Result<(), ContractError> {
    contract.tokens.remove(storage, &token_id)?; // the tokens map is a propertie of the cw721 base contract, and it holds the information about existing stored tokens
    contract.decrement_tokens(storage)?; //  the cw721 base contract have a propertie called token_count that tracks the amout of tokens stored. The decrement_tokens() is a helper function to reduce by one the existing number
    BURNED.save(storage, token_id, &true)?; // save the the token_id in the map 
    Ok(())
  }
  
  // This function makes sure that only the address set as the minter is allowed to store, and that # of current stored NFTs is less or equal the # defined as max supply
  pub fn can_store(
    deps: &DepsMut,
    info: &MessageInfo
  ) -> Result<(), ContractError> {
    can_update(deps, info)?;  // makes sure that only the address set as the minter is allowed to store
  
    let config = CONFIG.load(deps.storage)?; // loads the contract config
    if config.token_total >= config.token_supply {
        return Err(ContractError::MaxTokenSupply {});
    }
  
    Ok(())
  }
  
  pub fn can_pay(
    config: &Config,
    info: &MessageInfo,
    amount: Uint128
  ) -> Result<Coin, ContractError> {
    let mut coin_found: Coin = Coin::new(0, "none");
  
    if let Some(coin) = info.funds.first() { // DOCS -> The funds that are sent to the contract as part of `MsgInstantiateContract` or `MsgExecuteContract`. The transfer is processed in bank before the contract
                                                //  is executed such that the new balance is visible during contract execution.
        if coin.denom != config.cost_denom {
          Err(ContractError::WrongToken {})
        } else {
            let total = config.cost_amount * amount;
            println!("NotEnoughFunds: {} {} {}", coin.amount < total, coin.amount, total);
            if coin.amount < total {
                return Err(ContractError::NotEnoughFunds {});
            }
  
            if coin.amount == total {
                coin_found.denom = coin.denom.clone();
                coin_found.amount = coin.amount;
  
                Ok(coin_found)
            } else {
              Err(ContractError::IncorrectFunds {})
            }
        }
    } else {
      Err(ContractError::NoFundsSent {})
    }
  }
  
  pub fn can_mint(
    count: &u64,
    time: &Timestamp,
    start_mint: &Option<Timestamp>,
    token_total: Uint128,
    token_supply: Uint128,
    minter: &Addr,
    sender: &Addr
  ) -> Result<Uint128, ContractError> {
    if token_total == Uint128::from(0u32) {
        return Err(ContractError::CantMintNothing {});
    }
  
    if let Some(stamp) = start_mint {
      if time < stamp {
        return Err(ContractError::CantMintYet {})
      }
    }
  
    let current_count = Uint128::from(*count);
  
    if current_count == token_supply {
      return Err(ContractError::MaxTokenSupply {});
    }
  
    if current_count == token_total {
        return Err(ContractError::MaxTokens {});
    }
  
    // dont allow contract minter to become owner of tokens
    if sender == minter {
        return Err(ContractError::Unauthorized {})
    }
  
    Ok(current_count)
  }
  
  pub fn update_total(
    storage: &mut dyn Storage,
    amount: Uint128
  ) -> Result<(), ContractError> {
    let mut config = CONFIG.load(storage)?;
    config.token_total += amount;
    CONFIG.save(storage, &config)?;
    Ok(())
  }
  
  pub fn try_store(
    storage: &mut dyn Storage,
    nft_data: &MintMsg<Extension>,
    minter: &Addr,
    contract: &CW721Contract,
  ) -> Result<(), ContractError> {
    let token_id = nft_data.token_id.clone();
  
    // create the token
    let token = TokenInfo {
        owner: minter.clone(),
        approvals: vec![],
        token_uri: nft_data.token_uri.clone(),
        extension: nft_data.extension.clone(),
    };
  
    contract.tokens.save(storage, &token_id, &token)?;
  
    Ok(())
  }
  
  pub fn try_mint(
    storage: &mut dyn Storage,
    sender: &Addr,
    minter: &Addr,
    contract: &CW721Contract,
    current: &String
  ) -> Result<(), ContractError> {
    let old_token = contract.tokens.load(storage, current)?;
    if old_token.owner != minter.clone() {
      return Err(ContractError::Claimed {})
    }
    let mut new_token = old_token.clone();
    new_token.owner = sender.clone();
    contract.tokens.replace(storage, current, Some(&new_token), Some(&old_token))?;
    contract.increment_tokens(storage)?;
    Ok(())
  }