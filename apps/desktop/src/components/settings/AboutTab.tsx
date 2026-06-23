import { useI18n } from "../../lib/i18n";
import type { TranslationKey } from "../../lib/i18n";
import type { ModelInfo } from "../../lib/types";

interface AboutTabProps {
  version: string;
  providers: ModelInfo[];
}

const FEATURE_KEY_MAP: Record<string, TranslationKey> = {
  stt: "settings.feature.stt",
  ocr: "settings.feature.ocr",
  formatter: "settings.feature.formatter",
  tts: "settings.feature.tts",
};

export function AboutTab({ version, providers }: AboutTabProps) {
  const { t } = useI18n();

  return (
    <>
      {/* Identity / wordmark */}
      <div className="settings-section">
        <div>
          <span>exoquill</span>{" "}
          <span className="settings-about__version">
            {t("settings.about.version")} {version}
          </span>
        </div>
        <p className="settings-about__tagline">{t("settings.about.tagline")}</p>
      </div>

      {/* License */}
      <div className="settings-section">
        <h2 className="settings-section__title">{t("settings.about.licenseTitle")}</h2>
        <p className="settings-about__body">{t("settings.about.licenseBody")}</p>
      </div>

      {/* On-device components */}
      <div className="settings-section">
        <h2 className="settings-section__title">{t("settings.about.componentsTitle")}</h2>
        {providers.map((provider) => {
          const featureKey = FEATURE_KEY_MAP[provider.feature];
          const featureLabel = featureKey ? t(featureKey) : provider.feature;

          return (
            <div className="settings-provider" key={provider.providerId}>
              <div>
                <div className="settings-provider__name">
                  {provider.displayName}
                  {provider.version !== "" && (
                    <span
                      style={{ color: "var(--text-muted)", marginLeft: "0.4em" }}
                    >
                      {provider.version}
                    </span>
                  )}
                </div>
                <div className="settings-provider__feature">{featureLabel}</div>
              </div>
              <span className="settings-badge">{provider.runtimeLicense}</span>
            </div>
          );
        })}
      </div>

      {/* Credits */}
      <div className="settings-section">
        <h2 className="settings-section__title">{t("settings.about.creditsTitle")}</h2>
        <p className="settings-about__body">{t("settings.about.creditsBody")}</p>
      </div>
    </>
  );
}
