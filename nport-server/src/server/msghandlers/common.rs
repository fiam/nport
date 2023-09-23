use crate::server::{hostname, state::SharedState};

pub enum ValidationError {
    // Contains invalid characters
    Invalid,
    // Not allowed because of it's used with something else or blacklisted
    Disallowed,
}

pub fn normalized_client_subdomain(state: &SharedState, subdomain: &str) -> String {
    // Strip main domain, if it ends with it
    let subdomain = subdomain.to_lowercase();
    let suffix = format!(".{}", state.hostnames().domain());
    subdomain
        .strip_suffix(&suffix)
        .unwrap_or(&subdomain)
        .to_string()
}

pub fn subdomain_for_forwarding(
    state: &SharedState,
    requested_hostname: Option<&str>,
) -> Result<Option<String>, ValidationError> {
    let requested_hostname = requested_hostname.unwrap_or_default();
    if requested_hostname.is_empty() {
        return Ok(None);
    }
    // Strip main domain, if it ends with it
    let requested_hostname = normalized_client_subdomain(state, &requested_hostname);

    // Now validate the rest
    if !hostname::is_valid(&requested_hostname) || requested_hostname.contains('.') {
        return Err(ValidationError::Invalid);
    }
    if state.hostnames().is_subdomain_used(&requested_hostname) {
        return Err(ValidationError::Disallowed);
    }
    if state.hostnames().is_subdomain_blocked(&requested_hostname) {
        return Err(ValidationError::Disallowed);
    }
    Ok(Some(requested_hostname))
}
