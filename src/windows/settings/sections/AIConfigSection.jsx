import '@tabler/icons-webfont/dist/tabler-icons.min.css';
import { useTranslation } from 'react-i18next';
import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from '@shared/store/toastStore';
import SettingsSection from '../components/SettingsSection';
import SettingItem from '../components/SettingItem';
import Input from '@shared/components/ui/Input';
import Select from '@shared/components/ui/Select';
import Button from '@shared/components/ui/Button';
function AIConfigSection({
  settings,
  onSettingChange
}) {
  const {
    t
  } = useTranslation();
  const [testing, setTesting] = useState(false);
  // 推荐模型必须是视觉模型：Qwen2-7B-Instruct 会被后端 ensure_vision_model 拒绝
  // （含 instruct 且不含 vision/vl），列为推荐会让用户必然测试失败。
  const modelOptions = [{
    value: 'Qwen/Qwen2.5-VL-7B-Instruct',
    label: 'Qwen2.5-VL-7B-Instruct (推荐)'
  }, {
    value: 'deepseek-v3',
    label: 'DeepSeek V3'
  }, {
    value: 'qwen-turbo',
    label: '通义千问 Turbo'
  }, {
    value: 'chatglm3-6b',
    label: 'ChatGLM3-6B'
  }, {
    value: 'yi-34b-chat',
    label: 'Yi-34B-Chat'
  }];
  const handleTestConfig = async () => {
    setTesting(true);
    try {
      await invoke('test_screenshot_ai_config');
      toast.success(t('settings.aiConfig.testSuccess'));
    } catch (error) {
      toast.error(t('settings.aiConfig.testFailed', { error: String(error) }));
    } finally {
      setTesting(false);
    }
  };
  return <SettingsSection title={t('settings.aiConfig.title')} description={t('settings.aiConfig.description')}>
      <SettingItem label={t('settings.aiConfig.apiKey')} description={t('settings.aiConfig.apiKeyDesc')}>
        <Input type="password" value={settings.aiApiKey || ''} onChange={e => onSettingChange('aiApiKey', e.target.value)} placeholder={t('settings.aiConfig.apiKeyPlaceholder')} className="w-80" />
      </SettingItem>

      <SettingItem label={t('settings.aiConfig.model')} description={t('settings.aiConfig.modelDesc')}>
        <Select value={settings.aiModel} onChange={value => onSettingChange('aiModel', value)} options={modelOptions} className="w-64" />
      </SettingItem>

      <SettingItem label={t('settings.aiConfig.baseUrl')} description={t('settings.aiConfig.baseUrlDesc')}>
        <Input type="text" value={settings.aiBaseUrl || ''} onChange={e => onSettingChange('aiBaseUrl', e.target.value)} placeholder="https://api.siliconflow.cn/v1" className="w-80" />
      </SettingItem>

      <SettingItem label={t('settings.aiConfig.test')} description={t('settings.aiConfig.testDesc')}>
        <Button onClick={handleTestConfig} loading={testing} icon={<i className="ti ti-test-pipe"></i>}>
          {t('settings.aiConfig.testButton')}
        </Button>
      </SettingItem>
    </SettingsSection>;
}
export default AIConfigSection;