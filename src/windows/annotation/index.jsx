import { createRoot } from 'react-dom/client';
import App from './App';
import '@shared/i18n';
import '@unocss/reset/tailwind.css';
import 'virtual:uno.css';
import '@shared/styles/index.css';
import { initSettings } from '@shared/store/settingsStore';

// 图像编辑器是独立窗口，不经过主窗口的启动流程。shared/i18n.js 初始语言
// 固定为 zh-CN，若不加载设置并切换语言，用户在设置里切到 en-US 后编辑器
// 按钮文案仍会是中文。先异步同步语言再渲染，避免启动闪烁。
const root = createRoot(document.getElementById('root'));

initSettings()
  .catch((error) => console.error('加载图像编辑器语言设置失败:', error))
  .finally(() => {
    root.render(<App />);
  });