use cosmwasm_std::{
    Env,
    DepsMut,
    MessageInfo,
    Response,
    BankMsg,
    CosmosMsg,
    Uint128,
};

use cw721_base::{ MintMsg };
use cw721_base::state::{ TokenInfo };

use crate::state::{
    CW721Contract,
    Extension,
    CONFIG,
    Metadata,
    Trait,
};

use crate::helpers::{
    _can_mint,
    _can_pay,
    _can_store,
    _can_update,
    _try_mint,
    _try_store,
    __update_total,
    __burn_token,
    __update_burnt_amount,
    __update_burnt_list
};

use crate::error::ContractError;

use crate::msg::{ BatchStoreMsg, BatchMintMsg, StoreConfMsg };

ANDRE: THIS FUNCTION EXECUTES THE BURN OF A cw721 TOKEN
pub fn execute_burn(
    deps: DepsMut,
    info: MessageInfo,
    token_id: String,   ANDRE: THIS PARAMETER IDENTIFIES THE THE TOKEN TO BE BURNED
) -> Result<Response, ContractError> {
    let cw721_contract = CW721Contract::default(); ANDRE: LOADS THE INTERFACE TO HANDLE OPERATIONS??
    let config = CONFIG.load(deps.storage)?; ANDRE: LOADS THE STORED CONFIGURATION

    ANDRE: THIS IF() HANDLES THE BURN DONE BY THE OWNER OF THE TOKEN
    if config.owners_can_burn == true {
        let token = cw721_contract.tokens.load(deps.storage, &token_id)?; -> ANDRE: TRIES TO LOAD THE SPECIFIED CW721 TOKEN

        ANDRE: VALIDATES IF THE ADDRESS OF THE SPECIFIED TOKEN OWNER IS THE ADDR TRYING TO EXECUTE THE BURN
        if token.owner != info.sender { ANDRE: MAYBE THIS VALIDATION CAN BE DONE INSIDE THE _can_update(), THAT IS USED TO VERIFY IF THE MINTER IS THE ONE CALLING THE CONTRACT
            return Err(ContractError::Unauthorized {}) ANDRE: MAYBE A SPECIFIC ERROR MESSAGE CAN BE USED HERE?
        }

        ANDRE: THE NEXT 3 LINES OF CODE WILL PERFORM CHANGES TO THE CONST VARIABLES CREATED IN THE state.rs FILE. 
        THIS FUNCTIONS LIVE ON THE helpers.rs file, WHICH WILL BE ANALYSED IN THE FUTURE
        __burn_token(&cw721_contract, deps.storage, token_id.clone())?; 

        __update_burnt_amount(deps.storage, &info.sender)?;

        __update_burnt_list(deps.storage, &info.sender, &token_id)?;

        return Ok(Response::new()
            .add_attribute("action", "burn")
            .add_attribute("type", "owner_burn")
            .add_attribute("token_id", token_id))
    }

    ANDRE: THIS IF() HANDLES THE BURN DONE BY THE MINTER OF THE CONTRACT
    if config.minter_can_burn == true {
        // validate sender permissions
        ANDRE: THE NEXT 3 LINES OF CODE WILL PERFORM CHANGES TO THE CONST VARIABLES CREATED IN THE state.rs FILE. 
        THIS FUNCTIONS LIVE ON THE helpers.rs file, WHICH WILL BE ANALYSED IN THE FUTURE
        _can_update(&deps, &info)?; ANDRE: CHECKS IF THE EXECUTE ADDRESS IN THE MINTER

        __burn_token(&cw721_contract, deps.storage, token_id.clone())?;

        ANDRE: THE CALL TO UPDATE THE BURNT AMOUNT (__update_burnt_amount) IS NOT DONE, PROBABLY BECAUSE
        THERE IS NO NEED TO KEEP TRACK OF TOKENS BURNT BY THE MINTER???
        __update_burnt_list(deps.storage, &info.sender, &token_id)?;

        return Ok(Response::new()
            .add_attribute("action", "burn")
            .add_attribute("type", "minter_burn")
            .add_attribute("token_id", token_id))
    }

    Ok(Response::new()
        .add_attribute("action", "burn_nothing")
        .add_attribute("why", "configuration")
        .add_attribute("owners_can_burn", config.owners_can_burn.to_string())
        .add_attribute("minter_can_burn", config.minter_can_burn.to_string())
    )
}

pub fn execute_burn_batch(
    deps: DepsMut,
    info: MessageInfo,
    tokens: Vec<String>
) -> Result<Response, ContractError> {
    let cw721_contract = CW721Contract::default(); ANDRE: LOADS THE CW721 INTERFACE TO HANDLE OPERATIONS??
    let config = CONFIG.load(deps.storage)?;

    ANDRE: THE NEXT 2 IF CLAUSES, CHECKS IF THE AMOUNT OF TOKENS TO BE BURNT ARE BETWEEN 1 AND 30
    MAYBE THEY CAN BE ENCAPSULATED IN A FUNCTION IN THE helpers.rs FILE
    MAYBE INCLUDE THIS RANGE (1..31) AS A PARAMETER IN THE CONFIG FILE, SO THAT IT CAN BE UPDATED OVER TIME?
    if tokens.len() > 30 {
        return Err(ContractError::RequestTooLarge{ size: tokens.len() })
    }

    if tokens.len() == 0 {
        return Err(ContractError::RequestTooSmall{ size: tokens.len() })
    }

    ANDRE: THIS IF CLAUSE HANDLES THE BURN LIST OF THE TOKENS DONE BY THE OWNER OF THE TOKENS
    if config.owners_can_burn == true {
        let mut burnt_tokens = vec![];

        for token_id in tokens {
            let token = cw721_contract.tokens.load(deps.storage, &token_id)?;

            ANDRE: IF THE LAST TOKEN IN THE LIST DONT PASS THIS IF, ALL OF THE PREVIOUS ACTIONS WILL BE REVERTED (NOT SURE)
            IF THAT IS THE CASE, AND IF WE WANT TO KEEP THE LOGIC OF BURN ALL OR NONE, WE SHOULD EXTRACT THIS VERIFICATION TO BE DONE BEFORE THIS FOR LOOP
            IT WILL PROBABLY OPTIMIZE THE CODE FOR A CASE WHERE THE SENDER IS TRYING TO BURN A TOKEN THAT DOES NOT BELONG TO HIM
            if token.owner != info.sender {
                return Err(ContractError::Unauthorized {})
            }

            __burn_token(&cw721_contract, deps.storage, token_id.clone())?;

            __update_burnt_amount(deps.storage, &info.sender)?;

            __update_burnt_list(deps.storage, &info.sender, &token_id)?;

            burnt_tokens.push(token_id);
        }

        return Ok(Response::new()
            .add_attribute("action", "burn_batch")
            .add_attribute("type", "owner_burn")
            .add_attribute("tokens", String::from(format!("[{}]", burnt_tokens.join(","))))
        )
    }

    ANDRE: THIS IF CLAUSE HANDLES THE BURN LIST OF TOKENS DONE BY THE mINTER OF THE TOKENS
    if config.minter_can_burn == true {
        // validate sender permissions
        _can_update(&deps, &info)?;

        let mut burnt_tokens = vec![]; ANDRE: DECLARE ONLY ONCE BEFORE THE IF CLAUSES, BECAUSE IT WILL ONLY BE USED IN ONE OF THE CLAUSES
                                            WHY DO WE NEED THIS VECTOR? CANT WE USE THE VECTOR THAT IS PASSES IN THE FUNCTION SIGNATURE?
                                            THIS IS ONLY USED TO COMPOSE THE RESPONSE...

        for token_id in tokens {
            __burn_token(&cw721_contract, deps.storage, token_id.clone())?;

            __update_burnt_list(deps.storage, &info.sender, &token_id)?;

            burnt_tokens.push(token_id);
        }

        return Ok(Response::new()
            .add_attribute("action", "burn")
            .add_attribute("type", "minter_burn")
            .add_attribute("tokens", String::from(format!("[{}]", burnt_tokens.join(","))))
        )
    }

    Ok(Response::new()
        .add_attribute("action", "burn_nothing")
        .add_attribute("why", "configuration")
        .add_attribute("owners_can_burn", config.owners_can_burn.to_string())
        .add_attribute("minter_can_burn", config.minter_can_burn.to_string())
    )
}

pub fn execute_mint(
    env: Env,
    deps: DepsMut,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let cw721_contract = CW721Contract::default(); ANDRE: LOADS THE CW721 INTERFACE TO HANDLE OPERATIONS??
    let config = CONFIG.load(deps.storage)?;
    let minter = cw721_contract.minter.load(deps.storage)?; ANDRE: LOADS THE MINTER ADDRESS
    let current_count = cw721_contract.token_count(deps.storage)?; ANDRE: LOADS THE COUNT OF ALREADY MINTED TOKENS

    // check if we can mint
    ANDRE: THIS FUNCTION VALIDATES THAT A MINT CAN HAPPEN
    let current_token_id = _can_mint(
        &current_count,
        &env.block.time,
        &config.start_mint,
        config.token_total,
        config.token_supply,
        &minter,
        &info.sender
    )?;

    // validate funds according to set price
    let coin_found = _can_pay(&config, &info, Uint128::from(1u32))?; VALIDATES THAT THE EXECUTER HAVE FUNDS TO PAY THE MINT

    ANDRE: TRY TO EXECTUTE THE MINT
    _try_mint(
        deps.storage,
        &info.sender,
        &minter,
        &cw721_contract,
        &current_token_id.to_string()
    )?;

    // send funds to the configured funds wallet
    // send the info below
    Ok(Response::new()
        .add_attribute("action", "mint")
        .add_attribute("owner", info.sender)
        .add_attribute("token_id", current_count.to_string()) ANDRE: THE COUNT IS THE TOKEN_ID ???
        .add_message(
            ANDRE: THIS MESSAGE IS SENT TO THE BANK MODULE, SO THAT THE EXECUTER PAY FOR THE MINT
            IS THE EXECUTER ONLY ALLOWED TO PAY IN THE NATIVE TOKEN? IF SO, HOW COULD WE ALLOW ALSO CW20 TOKENS
            CosmosMsg::Bank(BankMsg::Send {
                to_address: config.funds_wallet.to_string(),
                amount: vec![coin_found],
            }.clone())
        )
    )
}

pub fn execute_mint_batch(
    env: Env,
    deps: DepsMut,
    info: MessageInfo,
    msg: BatchMintMsg,
) -> Result<Response, ContractError> {
    let cw721_contract = CW721Contract::default(); ANDRE: LOADS THE CW721 INTERFACE TO HANDLE OPERATIONS??

    let config = CONFIG.load(deps.storage)?;

    let minted_total = cw721_contract.token_count(deps.storage)?; ANDRE: LOADS THE CURRENT MINT TOTAL
    let minter = cw721_contract.minter.load(deps.storage)?; ANDRE: LOADS THE MINTER DEFINED IN THE CONTRACT

    let mut mint_amount = msg.amount; ANDRE: ASSIGNS THE MESSAGE PROPERTY AMMOUNT TO A MUT VARIABLE

    ANDRE: IF BatchMintMsg ARRIVES WITH A 0 AS VALUE, WHY CHANGET IT TO 1?? AN ERROR SHOULD BE THROWN
    if mint_amount == Uint128::from(0u32) {
        mint_amount = Uint128::from(1u32)
    }

    ANDRE: IF BatchMintMsg ARRIVES WITH A VALUE GRATER THEN THE ALLOWED MINT SIZE, WHY CHANGET IT TO THE MAX?? AN ERROR SHOULD BE THROWN
    if mint_amount > config.max_mint_batch {
        mint_amount = config.max_mint_batch
    }

    // check if we can mint
    ANDRE: THIS FUNCTION IS PART OF helpers.rs FILE, THAT WILL BE ANALISED IN A FUTURE CODE JOURNAL
    let mut current_token_id = _can_mint(
        &minted_total,
        &env.block.time,
        &config.start_mint,
        config.token_total,
        config.token_supply,
        &minter,
        &info.sender
    )?;

    // validate funds according to set price and total to mint
    ANDRE: THIS FUNCTION IS PART OF helpers.rs FILE, THAT WILL BE ANALISED IN A FUTURE CODE JOURNAL
    let mut coin_found = _can_pay(&config, &info, mint_amount)?;

    ANDRE: VARIABLE CREATED TO KEEP TRACK OF TOTAL MINTED TOKENS
    let mut total_minted = 0u32;

    ANDRE: VECTOR THAT WILL CONTAIN THE IDS OF MINTED TOKENS, TO BE USED IN THE RESPONSE
    let mut ids: Vec<String> = vec![];

    ANDRE: WHILE LOOP THAT WILL MINT AND UPDATE THE TRACKING VARIABLES STATE
    while Uint128::from(total_minted) < mint_amount {
        ANDRE: THIS FUNCTION IS PART OF helpers.rs FILE, THAT WILL BE ANALISED IN A FUTURE CODE JOURNAL
        let res = _try_mint(
            deps.storage,
            &info.sender,
            &minter,
            &cw721_contract,
            &current_token_id.to_string()
        );

        ANDRE: CHECKS IF THE PREVIOUS CALL IS SUCCEFULL AND THEN UPDATES THE TRACKING VARIABLES
        if res.is_ok() {
            total_minted += 1;
            current_token_id += Uint128::from(1u32);
            ids.push(current_token_id.to_string())
        }
    }

    ANDRE: WHY DO WE NEED TO UPDATE THIS VARIABLE??? tHE RESPONSE FORM THE _can_pay() SHOULD ALREADY CONTAIN THE CORRECT VALUE
    coin_found.amount = config.cost_amount * Uint128::from(total_minted);

    // send funds to the configured funds wallet
    // send the info below
    Ok(Response::new()
        .add_attribute("action", "mint_batch")
        .add_attribute("owner", info.sender)
        .add_attribute("requested", msg.amount.to_string())
        .add_attribute("minted", total_minted.to_string())
        .add_attribute("cost", coin_found.amount.to_string())
        .add_attribute("list", String::from(format!("[{}]", ids.join(","))))
        .add_message(
            ANDRE: THIS MESSAGE IS SENT TO THE BANK MODULE, SO THAT THE EXECUTER PAY FOR THE BATCH MINT
            CosmosMsg::Bank(BankMsg::Send {
                to_address: config.funds_wallet.to_string(),
                amount: vec![coin_found],
            }.clone())
        )
    )
}

ANDRE: THIS FUNCTION STORES THE NFT ON CHAIN - I THINK THIS IS A PRE MINT STEP. NOT SURE...
pub fn execute_store(
    deps: DepsMut,
    info: MessageInfo,
    nft_data: MintMsg<Extension>,
) -> Result<Response, ContractError> {
    // validate sender permissions
    ANDRE: THIS FUNCTION IS PART OF helpers.rs FILE, THAT WILL BE ANALISED IN A FUTURE CODE JOURNAL
    _can_store(&deps, &info)?;

    let cw721_contract = CW721Contract::default(); ANDRE: LOADS THE CW721 INTERFACE TO HANDLE OPERATIONS??
    let minter = cw721_contract.minter.load(deps.storage)?; ANDRE: LOADS THE MINTER DEFINED IN THE CONTRACT

    ANDRE: THIS FUNCTION IS PART OF helpers.rs FILE, THAT WILL BE ANALISED IN A FUTURE CODE JOURNAL
    _try_store(deps.storage, &nft_data, &minter, &cw721_contract)?;

    ANDRE: WHY NOT USE THE UPDATE FUNCTION ON THE CONFIG, INSTEAD OF LOADING AND UPDATING, AND THE SAVING
    let total = CONFIG.load(deps.storage)?.token_total + Uint128::from(1u8);
    __update_total(deps.storage, total)?;

    Ok(Response::new()
        .add_attribute("action", "store")
        .add_attribute("token_total", total.to_string())
    )
}

NDRE: THIS FUNCTION STORES A BATCH OF NFTs ON CHAIN - I THINK THIS IS A PRE MINT STEP. NOT SURE...
pub fn execute_store_batch(
    deps: DepsMut,
    info: MessageInfo,
    data: BatchStoreMsg,
) -> Result<Response, ContractError> {
    // validate sender permissions
    _can_store(&deps, &info)?;

    let cw721_contract = CW721Contract::default(); ANDRE: CREATES THE INTERFACE
    let minter = cw721_contract.minter.load(deps.storage)?; ANDRE: LOAD THE MINTER INFORMATION

    let mut total = CONFIG.load(deps.storage)?.token_total;
    ANDRE: THIS FUNCTION IS VERY SIMILAR TO THE PREVIOUS ONE, EXCEPT THIS FOR LOOP
    for nft_data in data.batch {
        _try_store(deps.storage, &nft_data, &minter, &cw721_contract)?;
        ANDRE: INSTEAD OF UPDATING THE TOTAL VARIABLE, WHY DON'T USE DIRECTLY THE data LENGHT??
        total += Uint128::from(1u8)
    }

    __update_total(deps.storage, total)?;

    Ok(Response::new()
        .add_attribute("action", "store_batch")
        .add_attribute("token_total", total.to_string())
    )
}

pub fn execute_store_conf(
    deps: DepsMut,
    info: MessageInfo,
    msg: StoreConfMsg,
)-> Result<Response, ContractError> {
    // validate sender permissions
    _can_store(&deps, &info)?;

    let cw721_contract = CW721Contract::default();
    let minter = cw721_contract.minter.load(deps.storage)?;

    let mut config = CONFIG.load(deps.storage)?.store_conf;
    if config.is_none() && msg.conf.is_none() {
        return Err(ContractError::NoConfiguration {})
    }

    if msg.conf.is_some() {
        config = msg.conf
    }

    let conf = config.unwrap();

    let mut total = CONFIG.load(deps.storage)?.token_total;

    for attr_values in msg.attributes {
        let name = format!("{} #{}", conf.name, total);

        let mut attr : Vec<Trait> = vec![];
        for (index, value) in attr_values.iter().enumerate() {
            attr.push(Trait {
                display_type: None,
                value: value.clone(),
                trait_type: conf.attributes[index].clone()
            })
        }

        let token = TokenInfo {
            owner: minter.clone(),
            approvals: vec![],
            token_uri: None,
            extension: Some(Metadata {
                name: Some(name.clone()),
                description: Some(format!("{}", conf.desc)),
                image: Some(format!("{}/{}.png", conf.ipfs, total)),
                attributes: Some(attr),
                animation_url: None,
                background_color: None,
                image_data: None,
                external_url: None,
                youtube_url: None,
            })
        };

        cw721_contract.tokens.save(deps.storage, &total.to_string(), &token)?;

        total += Uint128::from(1u8)
    }

    __update_total(deps.storage, total)?;

    Ok(Response::new()
        .add_attribute("action", "store_conf")
        .add_attribute("token_total", total.to_string())
    )
}
