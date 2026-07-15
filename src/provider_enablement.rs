// SPDX-License-Identifier: MPL-2.0

use crate::config::{Config, ProviderEnablement};
use crate::detection::DetectionSnapshot;
use crate::model::ProviderId;
use crate::providers::registry;

#[must_use]
pub fn provider_enabled(
    config: &Config,
    detection: &DetectionSnapshot,
    provider: ProviderId,
) -> bool {
    resolve(
        config.provider_enablement(provider),
        detection.detected(provider),
        !registry::discover_accounts(provider, config).is_empty(),
    )
}

#[must_use]
pub fn resolve(enablement: ProviderEnablement, detected: bool, has_account: bool) -> bool {
    match enablement {
        ProviderEnablement::Auto => detected || has_account,
        ProviderEnablement::Enabled => true,
        ProviderEnablement::Disabled => false,
    }
}

#[cfg(test)]
mod tests {
    use super::resolve;
    use crate::config::ProviderEnablement;

    #[test]
    fn auto_enablement_uses_detection_or_account_presence() {
        assert!(resolve(ProviderEnablement::Auto, true, false));
        assert!(resolve(ProviderEnablement::Auto, false, true));
        assert!(!resolve(ProviderEnablement::Auto, false, false));
    }

    #[test]
    fn explicit_enablement_overrides_detection_and_account_presence() {
        assert!(resolve(ProviderEnablement::Enabled, false, false));
        assert!(!resolve(ProviderEnablement::Disabled, true, true));
    }

    #[test]
    fn fresh_config_without_detection_enables_no_providers() {
        let config = crate::config::Config::default();
        let detection = crate::detection::DetectionSnapshot::default();
        for provider in crate::model::ProviderId::ALL {
            assert!(!super::provider_enabled(&config, &detection, provider));
        }
    }
}
