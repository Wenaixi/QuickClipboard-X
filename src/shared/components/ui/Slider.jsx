import { useState, useEffect } from 'react';
function Slider({
  value,
  onChange,
  min = 0,
  max = 100,
  step = 1,
  unit = '',
  className = '',
  sliderClassName = ''
}) {
  const [displayValue, setDisplayValue] = useState(value);
  const [isDragging, setIsDragging] = useState(false);
  
  useEffect(() => {
    setDisplayValue(value);
  }, [value]);
  
  const handleInput = e => {
    const newValue = parseFloat(e.target.value);
    setDisplayValue(newValue);
  };
  
  const handleMouseDown = () => {
    setIsDragging(true);
  };
  
  const handleMouseUp = () => {
    if (isDragging) {
      setIsDragging(false);
      onChange(displayValue);
    }
  };

  const handleTouchEnd = () => {
    if (isDragging) {
      setIsDragging(false);
      onChange(displayValue);
    }
  };

  // 键盘方向键/PageUp/Home 调整走 input 事件,没有对应的 mouseup——
  // 若不在 keyup 时提交,设置只在界面显示改变,实际不会写入设置,
  // 直到下一次鼠标拖动才误把上次的值提交上去。
  const handleKeyUp = () => {
    onChange(displayValue);
  };

  return <div className={`flex items-center justify-end gap-2 ${className}`}>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={displayValue}
        onInput={handleInput}
        onMouseDown={handleMouseDown}
        onMouseUp={handleMouseUp}
        onTouchEnd={handleTouchEnd}
        onKeyUp={handleKeyUp}
        className={`h-2 bg-qc-panel-2 rounded-lg appearance-none cursor-pointer accent-[var(--qc-accent)] ${sliderClassName || 'w-24'}`}
      />
      <span className="text-sm font-medium text-qc-fg-muted whitespace-nowrap">
        {displayValue}{unit}
      </span>
    </div>;
}
export default Slider;
