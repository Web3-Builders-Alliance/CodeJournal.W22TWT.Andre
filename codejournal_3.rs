use std::convert::TryInto;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, BankMsg, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    Response, StdResult, Uint128, Uint256, WasmMsg,
};
use cw2::set_contract_version;

use crate::error::ContractError;
use crate::msg::{ConfigResponse, ExecuteMsg, InstantiateMsg, LoanMsg, QueryMsg};
use crate::state::{CheckedLoanDenom, ADMIN, FEE, LOAN_DENOM, PROVISIONS, TOTAL_PROVIDED};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:cw-flash-loan";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // Validate an adress directly from the instatiate message in 1 line! 
    // Using the transpose() method to convert the Option of a Result into a Result of an Option!
    // This is necessary because addr_validate() returns a Result.
    let admin = msg.admin.map(|a| deps.api.addr_validate(&a)).transpose()?;

    // They use an implementation block on the Enum LoanDenom, to check the Denom type and do address validation if is CW-20
    // Because the instantiate entry point uses DepsMut, they use the unmutable Deps, because no change occurs inside the function
    // To achieve this, as_ref() is used. as_ref() transforms the DepsMut into Deps.
    let loan_denom = msg.loan_denom.into_checked(deps.as_ref())?;

    // In the next 4 lines of code, the contract sets the initial state on the different tracking items
    ADMIN.save(deps.storage, &admin)?;
    FEE.save(deps.storage, &msg.fee)?;
    LOAN_DENOM.save(deps.storage, &loan_denom)?;
    TOTAL_PROVIDED.save(deps.storage, &Uint128::zero())?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute(
            "admin",
            admin
                .map(|a| a.to_string())                     // Because admin is an Option of a Addr, this is a nice way to unwrap it into String if there is Some
                .unwrap_or_else(|| "None".to_string()),     // or use a default in case there is None
        )
        .add_attribute("fee", msg.fee.to_string()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        // This message just updates some config state on the contract, such as the admin and the fee for the contract
        ExecuteMsg::UpdateConfig { admin, fee } => {
            execute_update_config(deps, info.sender, admin, fee)
        }
        // This message is the one used to request a flash loan!
        ExecuteMsg::Loan { receiver, amount } => execute_loan(deps, env, receiver, amount),
        // Message used in the workflow of the previous message! It is used as a call back for the Loan, to ensure the balances are correct!
        ExecuteMsg::AssertBalance { amount } => execute_assert_balance(deps.as_ref(), env, amount),
        // Message used to provide any amount of native tokens into the vaults!
        ExecuteMsg::Provide {} => execute_provide_native(deps, env, info),
        // Message used to withdraw provided amounts in the vaults
        ExecuteMsg::Withdraw {} => execute_withdraw(deps, env, info),
        // Message used to provide any amount of cw20 tokens into the vaults!
        ExecuteMsg::Receive(cw20::Cw20ReceiveMsg {
            sender,
            amount,
            // Intentionally ignore message field. No additional
            // validation really to be done with this.
            msg: _,
        }) => execute_provide_cw20(deps, env, info, sender, amount),
    }
}

pub fn execute_update_config(
    deps: DepsMut,
    sender: Addr,
    new_admin: Option<String>,
    new_fee: Decimal,
) -> Result<Response, ContractError> {
    let admin = ADMIN.load(deps.storage)?;
    // with the following condition, it is ensured that only the defined admin can update de Config
    if Some(sender) != admin {
        return Err(ContractError::Unauthorized {});
    }

    let new_admin = new_admin.map(|a| deps.api.addr_validate(&a)).transpose()?;
    ADMIN.save(deps.storage, &new_admin)?;

    FEE.save(deps.storage, &new_fee)?;

    Ok(Response::new()
        .add_attribute("method", "update_config")
        .add_attribute(
            "new_admin",
            new_admin
                .map(|a| a.to_string())
                .unwrap_or_else(|| "None".to_string()),
        )
        .add_attribute("new_fee", new_fee.to_string()))
}

pub fn execute_loan(
    deps: DepsMut,
    env: Env,
    receiver: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let fee = FEE.load(deps.storage)?;
    let loan_denom = LOAN_DENOM.load(deps.storage)?;

    // gets the balance of a Denom in the contract
    let avaliable = query_avaliable_balance(deps.as_ref(), &env, &loan_denom)?;

    // depending if it is a native or a cw20 token, creates the message to send the funds to 
    // the contract that requested the loan
    let execute_msg = match loan_denom {
        CheckedLoanDenom::Cw20 { address } => WasmMsg::Execute {
            contract_addr: address.into_string(),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                amount,
                // they send a message in the transfer message, so that the receiving contract is notified
                // and can perform his own logic
                msg: to_binary(&LoanMsg::ReceiveLoan {})?,
                contract: receiver.clone(),
            })?,
            funds: vec![],
        },
        CheckedLoanDenom::Native { denom } => WasmMsg::Execute {
            contract_addr: receiver.clone(),
            // they send a message in the transfer message, so that the receiving contract is notified
            // and can perform his own logic
            msg: to_binary(&LoanMsg::ReceiveLoan {})?,
            funds: vec![Coin { amount, denom }],
        },
    };

    // Expect that we will get everything back plus the fee applied to
    // the amount borrowed. For example, if the contract holds 200
    // tokens and the fee is 0.03 a loan for 100 tokens should result
    // in 203 tokens being held by the contract.
    let expected = avaliable + (fee * amount);

    // Here they compose the second message to be included, where they execute a message in the contract itself, kinda of a callback
    let return_msg = WasmMsg::Execute {
        contract_addr: env.contract.address.into_string(),
        msg: to_binary(&ExecuteMsg::AssertBalance { amount: expected })?,
        funds: vec![],
    };

    Ok(Response::new()
        .add_attribute("method", "loan")
        .add_attribute("receiver", receiver)
        .add_message(execute_msg)
        .add_message(return_msg))
}

// This function is used to verify that the NATIVE token provided, is the correct one. Then the amount is sent back.
// The funds parameter is part of the InfoMessage
fn get_only_denom_amount(funds: Vec<Coin>, denom: String) -> Result<Uint128, ContractError> {
    if funds.len() != 1 {
        return Err(ContractError::WrongFunds { denom });
    }
    let provided = funds.into_iter().next().unwrap(); // takes the first position of the vector
    if provided.denom != denom {
        return Err(ContractError::WrongFunds { denom });
    }
    Ok(provided.amount)
}

pub fn execute_provide_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sender: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let loan_denom = LOAN_DENOM.load(deps.storage)?; // loads the Denom associated with this contract
    let loan_token = match &loan_denom {
        // this match makes sure the Denom is a cw20 and extracts the associated address
        CheckedLoanDenom::Cw20 { address } => address,
        CheckedLoanDenom::Native { .. } => return Err(ContractError::NativeExpected {}),
    };

    // this condition ensure that the provided cw20 token sent is the correct one!
    if *loan_token != info.sender {
        return Err(ContractError::Unauthorized {});
    }
    let sender = deps.api.addr_validate(&sender)?;

    let provided = amount;
    let balance = query_avaliable_balance(deps.as_ref(), &env, &loan_denom)? - provided; // balance is the amount held by the contract minus the amount of the loan
    let total_provied = TOTAL_PROVIDED.load(deps.storage)?; // loads the Item that tracks the total provided amount by the contract

    let amount_to_provide = if total_provied.is_zero() || balance.is_zero() {
        provided
    } else {
        // The amount you receive is (balance per provided) *
        // provided. This stops being able to withdraw and then
        // instantly deposit to drain the rewards from the contract.
        (total_provied.full_mul(provided) / Uint256::from_uint128(balance))
            .try_into()
            .unwrap()
    };

    PROVISIONS.update(deps.storage, sender.clone(), |old| -> StdResult<_> {
        Ok(old.unwrap_or_default().checked_add(amount_to_provide)?)
    })?;
    TOTAL_PROVIDED.update(deps.storage, |old| -> StdResult<_> {
        Ok(old.checked_add(amount_to_provide)?)
    })?;

    Ok(Response::new()
        .add_attribute("method", "provide_native")
        .add_attribute("provider", sender)
        .add_attribute("provided", amount_to_provide))
}

pub fn execute_provide_native(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let MessageInfo { sender, funds } = info;
    let loan_denom = LOAN_DENOM.load(deps.storage)?;
    let provided = match &loan_denom {
        CheckedLoanDenom::Cw20 { .. } => return Err(ContractError::Cw20Expected {}),
        CheckedLoanDenom::Native { denom } => get_only_denom_amount(funds, denom.clone())?,
    };

    // Need to use balance before we provided for this computaton as
    // our porportional entitlement needs to be set at the old rate.
    let balance = query_avaliable_balance(deps.as_ref(), &env, &loan_denom)? - provided;
    let total_provied = TOTAL_PROVIDED.load(deps.storage)?;

    let amount_to_provide = if total_provied.is_zero() || balance.is_zero() {
        provided
    } else {
        // The amount you receive is (balance per provided) *
        // provided. This stops being able to withdraw and then
        // instantly deposit to drain the rewards from the contract.
        (total_provied.full_mul(provided) / Uint256::from_uint128(balance))
            .try_into()
            .unwrap()
    };

    PROVISIONS.update(deps.storage, sender.clone(), |old| -> StdResult<_> {
        Ok(old.unwrap_or_default().checked_add(amount_to_provide)?)
    })?;
    TOTAL_PROVIDED.update(deps.storage, |old| -> StdResult<_> {
        Ok(old.checked_add(amount_to_provide)?)
    })?;

    Ok(Response::new()
        .add_attribute("method", "provide_native")
        .add_attribute("provider", sender)
        .add_attribute("provided", amount_to_provide))
}

fn compute_entitled(provided: Uint128, total_provided: Uint128, avaliable: Uint128) -> Uint128 {
    (avaliable.full_mul(provided) / Uint256::from_uint128(total_provided))
        .try_into()
        .unwrap()
}

pub fn execute_withdraw(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let MessageInfo { sender, .. } = info;
    let loan_denom = LOAN_DENOM.load(deps.storage)?;
    let total_provided = TOTAL_PROVIDED.load(deps.storage)?;

    let provided = if let Some(provision) = PROVISIONS.may_load(deps.storage, sender.clone())? {
        Ok(provision)
    } else {
        Err(ContractError::NoProvisions {})
    }?;

    let avaliable = query_avaliable_balance(deps.as_ref(), &env, &loan_denom)?;

    let entitled = compute_entitled(provided, total_provided, avaliable);

    PROVISIONS.save(deps.storage, sender.clone(), &Uint128::zero())?;
    TOTAL_PROVIDED.update(deps.storage, |old| -> StdResult<_> {
        Ok(old.checked_sub(provided)?)
    })?;

    let withdraw_message: CosmosMsg = match loan_denom {
        CheckedLoanDenom::Cw20 { address } => WasmMsg::Execute {
            contract_addr: address.into_string(),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Transfer {
                recipient: sender.to_string(),
                amount: entitled,
            })?,
            funds: vec![],
        }
        .into(),
        CheckedLoanDenom::Native { denom } => BankMsg::Send {
            to_address: sender.to_string(),
            amount: vec![Coin {
                amount: entitled,
                denom,
            }],
        }
        .into(),
    };

    Ok(Response::new()
        .add_attribute("method", "withdraw")
        .add_attribute("receiver", sender)
        .add_attribute("amount", entitled)
        .add_message(withdraw_message))
}

pub fn execute_assert_balance(
    deps: Deps,
    env: Env,
    expected: Uint128,
) -> Result<Response, ContractError> {
    let loan_denom = LOAN_DENOM.load(deps.storage)?;

    let avaliable = query_avaliable_balance(deps, &env, &loan_denom)?;

    if avaliable != expected {
        Err(ContractError::NotReturned {})
    } else {
        Ok(Response::new().add_attribute("method", "assert_balances"))
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetConfig {} => query_get_config(deps),
        QueryMsg::Provided { address } => query_provided(deps, address),
        QueryMsg::TotalProvided {} => query_total_provided(deps),
        QueryMsg::Entitled { address } => query_entitled(deps, env, address),
        QueryMsg::Balance {} => query_balance(deps, env),
    }
}

fn query_avaliable_balance(
    deps: Deps,
    env: &Env,
    loan_denom: &CheckedLoanDenom,
) -> StdResult<Uint128> {
    Ok(match loan_denom {
        CheckedLoanDenom::Cw20 { address } => {
            let balance: cw20::BalanceResponse = deps.querier.query_wasm_smart(
                address.to_string(),
                &cw20::Cw20QueryMsg::Balance {
                    address: env.contract.address.to_string(),
                },
            )?;
            balance.balance
        }
        CheckedLoanDenom::Native { denom } => {
            deps.querier
                .query_balance(&env.contract.address, denom)?
                .amount
        }
    })
}

pub fn query_get_config(deps: Deps) -> StdResult<Binary> {
    let admin = ADMIN.load(deps.storage)?;
    let fee = FEE.load(deps.storage)?;
    let loan_denom = LOAN_DENOM.load(deps.storage)?;

    to_binary(&ConfigResponse {
        admin: admin.map(|a| a.into()),
        fee,
        loan_denom,
    })
}

pub fn query_provided(deps: Deps, address: String) -> StdResult<Binary> {
    let address = deps.api.addr_validate(&address)?;
    let provided = PROVISIONS
        .may_load(deps.storage, address)
        .unwrap_or_default();

    match provided {
        Some(provided) => to_binary(&provided),
        None => to_binary(&Uint128::zero()),
    }
}

pub fn query_total_provided(deps: Deps) -> StdResult<Binary> {
    let total = TOTAL_PROVIDED.load(deps.storage)?;
    to_binary(&total)
}

pub fn query_entitled(deps: Deps, env: Env, address: String) -> StdResult<Binary> {
    let address = deps.api.addr_validate(&address)?;

    let loan_denom = LOAN_DENOM.load(deps.storage)?;
    let provided = PROVISIONS.may_load(deps.storage, address)?;

    match provided {
        Some(provided) => {
            let total_provided = TOTAL_PROVIDED.load(deps.storage)?;

            let avaliable = query_avaliable_balance(deps, &env, &loan_denom)?;

            let entitled = compute_entitled(provided, total_provided, avaliable);

            to_binary(&entitled)
        }
        None => to_binary(&Uint128::zero()),
    }
}

pub fn query_balance(deps: Deps, env: Env) -> StdResult<Binary> {
    let loan_denom = LOAN_DENOM.load(deps.storage)?;
    let avaliable = query_avaliable_balance(deps, &env, &loan_denom)?;

    to_binary(&avaliable)
}