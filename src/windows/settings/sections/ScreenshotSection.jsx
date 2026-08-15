import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { toast } from '@shared/store/toastStore';
import Button from '@shared/components/ui/Button';
import SettingsSection from '../components/SettingsSection';
import SettingItem from '../components/SettingItem';
import Toggle from '@shared/components/ui/Toggle';
import Select from '@shared/components/ui/Select';
import Input from '@shared/components/ui/Input';

function ScreenshotSection({
  settings,
  onSettingChange,
  onNavigateAiConfig
}) {
  const {
    t
  } = useTranslation();

  const elementDetectionOptions = [{
    value: 'none',
    label: t('settings.screenshot.detectionNone')
  }, {
    value: 'window',
    label: t('settings.screenshot.detectionWindow')
  }, {
    value: 'all',
    label: t('settings.screenshot.detectionAll')
  }];

  const lifecycleModeOptions = [{
    value: 'quick',
    label: `${t('settings.screenshot.lifecycleModeQuick')} - ${t('settings.screenshot.lifecycleModeQuickDesc')}`
  }, {
    value: 'dispose',
    label: `${t('settings.screenshot.lifecycleModeDispose')} - ${t('settings.screenshot.lifecycleModeDisposeDesc')}`
  }, {
    value: 'auto',
    label: `${t('settings.screenshot.lifecycleModeAuto')} - ${t('settings.screenshot.lifecycleModeAutoDesc')}`
  }];

  const lifecycleModeValue = settings.screenshotWindowLifecycleMode || 'quick';
  const [aiConfigured, setAiConfigured] = useState(false);
  const [testingAiConfig, setTestingAiConfig] = useState(false);

  useEffect(() => {
    let active = true;
    invoke('get_screenshot_ai_config_status')
      .then(value => { if (active) setAiConfigured(value === true); })
      .catch(() => { if (active) setAiConfigured(false); });
    return () => { active = false; };
  }, [settings.screenshotAiEnabled, settings.aiApiKey, settings.aiBaseUrl, settings.aiModel]);

  const testAiConfig = async () => {
    setTestingAiConfig(true);
    try {
      await invoke('test_screenshot_ai_config');
      setAiConfigured(true);
      toast.success(t('settings.screenshot.testAiConfigSuccess'));
    } catch (error) {
      toast.error(t('settings.screenshot.testAiConfigFailed', { error: String(error) }));
    } finally {
      setTestingAiConfig(false);
    }
  };

  return <SettingsSection title={t('settings.screenshot.title')} description={t('settings.screenshot.description')}>
      <SettingItem label={t('settings.screenshot.enabled')} description={t('settings.screenshot.enabledDesc')}>
        <Toggle checked={settings.screenshotEnabled} onChange={checked => onSettingChange('screenshotEnabled', checked)} />
      </SettingItem>

      <SettingItem label={t('settings.screenshot.elementDetection')} description={t('settings.screenshot.elementDetectionDesc')}>
        <Select value={settings.screenshotElementDetection || 'all'} onChange={value => onSettingChange('screenshotElementDetection', value)} options={elementDetectionOptions} className="w-48" />
      </SettingItem>

      <SettingItem label={t('settings.screenshot.magnifier')} description={t('settings.screenshot.magnifierDesc')}>
        <Toggle checked={settings.screenshotMagnifierEnabled} onChange={checked => onSettingChange('screenshotMagnifierEnabled', checked)} />
      </SettingItem>

      <SettingItem label={t('settings.screenshot.hints')} description={t('settings.screenshot.hintsDesc')}>
        <Toggle checked={settings.screenshotHintsEnabled} onChange={checked => onSettingChange('screenshotHintsEnabled', checked)} />
      </SettingItem>

      <SettingItem label={t('settings.screenshot.colorIncludeFormat')} description={t('settings.screenshot.colorIncludeFormatDesc')}>
        <Toggle checked={settings.screenshotColorIncludeFormat} onChange={checked => onSettingChange('screenshotColorIncludeFormat', checked)} />
      </SettingItem>

      <SettingItem label={t('settings.screenshot.aiEnabled')} description={t('settings.screenshot.aiEnabledDesc')}>
        <Toggle checked={settings.screenshotAiEnabled !== false} onChange={checked => onSettingChange('screenshotAiEnabled', checked)} />
      </SettingItem>

      <SettingItem label={t('settings.screenshot.aiPrompt')} description={t('settings.screenshot.aiPromptDesc')}>
        <Input value={settings.screenshotAiPrompt || ''} onChange={event => onSettingChange('screenshotAiPrompt', event.target.value)} className="w-80" />
      </SettingItem>

      <SettingItem label={t('settings.screenshot.aiConfigStatus')} description={aiConfigured ? t('settings.screenshot.aiConfigReady') : t('settings.screenshot.aiConfigMissing')}>
        {aiConfigured ? <Button variant="secondary" loading={testingAiConfig} onClick={testAiConfig} icon={<i className="ti ti-test-pipe" />}>
          {t('settings.screenshot.testAiConfig')}
        </Button> : <Button variant="secondary" onClick={onNavigateAiConfig} icon={<i className="ti ti-settings" />}>
          {t('settings.screenshot.configureAi')}
        </Button>}
      </SettingItem>

      <SettingItem label={t('settings.screenshot.lifecycleMode')} description={t('settings.screenshot.lifecycleModeDesc')}>
        <Select value={lifecycleModeValue} onChange={value => onSettingChange('screenshotWindowLifecycleMode', value)} options={lifecycleModeOptions} className="w-80" />
      </SettingItem>

      {lifecycleModeValue === 'auto' && <SettingItem label={t('settings.screenshot.autoDisposeMinutes')} description={t('settings.screenshot.autoDisposeMinutesDesc')}>
          <Input type="number" value={settings.screenshotAutoDisposeMinutes ?? 10} onChange={e => onSettingChange('screenshotAutoDisposeMinutes', parseInt(e.target.value) || 10)} min={1} max={1440} className="w-24" suffix={t('settings.screenshot.minutes')} />
        </SettingItem>}
    </SettingsSection>;
}
export default ScreenshotSection;