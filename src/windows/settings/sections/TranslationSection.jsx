import '@tabler/icons-webfont/dist/tabler-icons.min.css';
import { useTranslation } from 'react-i18next';
import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from '@shared/store/toastStore';
import SettingsSection from '../components/SettingsSection';
import SettingItem from '../components/SettingItem';
import Toggle from '@shared/components/ui/Toggle';
import Select from '@shared/components/ui/Select';
import Textarea from '@shared/components/ui/Textarea';
import Button from '@shared/components/ui/Button';
import Slider from '@shared/components/ui/Slider';
function TranslationSection({
  settings,
  onSettingChange
}) {
  const {
    t
  } = useTranslation();
  const [testing, setTesting] = useState(false);
  const targetLanguageOptions = [{
    value: 'auto',
    label: t('settings.translation.auto')
  }, {
    value: 'zh-CN',
    label: t('settings.translation.zhCN')
  }, {
    value: 'zh-TW',
    label: t('settings.translation.zhTW')
  }, {
    value: 'en',
    label: t('settings.translation.en')
  }, {
    value: 'ja',
    label: t('settings.translation.ja')
  }, {
    value: 'ko',
    label: t('settings.translation.ko')
  }, {
    value: 'fr',
    label: t('settings.translation.fr')
  }, {
    value: 'de',
    label: t('settings.translation.de')
  }, {
    value: 'es',
    label: t('settings.translation.es')
  }, {
    value: 'ru',
    label: t('settings.translation.ru')
  }];
  const outputModeOptions = [{
    value: 'stream',
    label: t('settings.translation.streamOutput')
  }, {
    value: 'paste',
    label: t('settings.translation.directPaste')
  }];
  const newlineModeOptions = [{
    value: 'auto',
    label: t('settings.translation.newlineAuto')
  }, {
    value: 'shift_enter',
    label: 'Shift+Enter'
  }, {
    value: 'enter',
    label: 'Enter'
  }, {
    value: 'unicode',
    label: t('settings.translation.newlineUnicode')
  }];
  const handleTestTranslation = async () => {
    setTesting(true);
    try {
      // 复用截图 AI 配置测试命令:后端 test_configuration 只依赖
      // ai_api_key/ai_base_url/ai_model(翻译与截图共享同一 AI 配置),
      // 对目标语言/提示词无依赖,可复用命令面免新增后端。
      await invoke('test_screenshot_ai_config');
      toast.success(t('settings.translation.testSuccess'));
    } catch (error) {
      toast.error(t('settings.translation.testFailed', { error: String(error) }));
    } finally {
      setTesting(false);
    }
  };
  return <>
      <SettingsSection title={t('settings.translation.title')} description={t('settings.translation.description')}>
        <SettingItem label={t('settings.translation.enabled')} description={t('settings.translation.enabledDesc')}>
          <Toggle checked={settings.aiTranslationEnabled} onChange={checked => onSettingChange('aiTranslationEnabled', checked)} />
        </SettingItem>

        <SettingItem label={t('settings.translation.targetLanguage')} description={t('settings.translation.targetLanguageDesc')}>
          <Select value={settings.aiTargetLanguage} onChange={value => onSettingChange('aiTargetLanguage', value)} options={targetLanguageOptions} className="w-56" />
        </SettingItem>

        <SettingItem label={t('settings.translation.translateOnCopy')} description={t('settings.translation.translateOnCopyDesc')}>
          <Toggle checked={settings.aiTranslateOnCopy} onChange={checked => onSettingChange('aiTranslateOnCopy', checked)} />
        </SettingItem>

        <SettingItem label={t('settings.translation.translateOnPaste')} description={t('settings.translation.translateOnPasteDesc')}>
          <Toggle checked={settings.aiTranslateOnPaste} onChange={checked => onSettingChange('aiTranslateOnPaste', checked)} />
        </SettingItem>

        <SettingItem label={t('settings.translation.inputSpeed')} description={t('settings.translation.inputSpeedDesc')}>
          <Slider value={settings.aiInputSpeed || 50} onChange={value => onSettingChange('aiInputSpeed', value)} min={10} max={100} step={5} unit={t('settings.translation.charsPerSecond')} className="w-64" />
        </SettingItem>

        <SettingItem label={t('settings.translation.outputMode')} description={t('settings.translation.outputModeDesc')}>
          <Select value={settings.aiOutputMode} onChange={value => onSettingChange('aiOutputMode', value)} options={outputModeOptions} className="w-56" />
        </SettingItem>

        <SettingItem label={t('settings.translation.newlineMode')} description={t('settings.translation.newlineModeDesc')}>
          <Select value={settings.aiNewlineMode} onChange={value => onSettingChange('aiNewlineMode', value)} options={newlineModeOptions} className="w-56" />
        </SettingItem>

        <SettingItem label={t('settings.translation.prompt')} description={t('settings.translation.promptDesc')}>
          <Textarea value={settings.aiTranslationPrompt || ''} onChange={e => onSettingChange('aiTranslationPrompt', e.target.value)} rows={3} placeholder={t('settings.translation.promptPlaceholder')} className="w-full" />
        </SettingItem>

        <SettingItem label={t('settings.translation.test')} description={t('settings.translation.testDesc')}>
          <Button onClick={handleTestTranslation} loading={testing} icon={<i className="ti ti-test-pipe"></i>}>
            {t('settings.translation.testButton')}
          </Button>
        </SettingItem>
      </SettingsSection>
    </>;
}
export default TranslationSection;