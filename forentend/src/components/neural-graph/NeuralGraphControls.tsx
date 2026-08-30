/**
 * Neural Graph controls — zoom/reset/fullscreen. Pure UI chrome; carries
 * no data of its own, only calls the callbacks `NeuralGraph.tsx` gives it.
 */
import React from 'react';
import { Maximize2, Minimize2, RotateCcw, ZoomIn, ZoomOut } from 'lucide-react';

interface NeuralGraphControlsProps {
  onZoomIn: () => void;
  onZoomOut: () => void;
  onReset: () => void;
  onToggleFullscreen: () => void;
  isFullscreen: boolean;
}

export const NeuralGraphControls: React.FC<NeuralGraphControlsProps> = ({ onZoomIn, onZoomOut, onReset, onToggleFullscreen, isFullscreen }) => {
  const btnClass =
    'p-1.5 rounded-md bg-[#020617] border border-slate-700 hover:border-cyan-600 text-slate-300 hover:text-cyan-300 cursor-pointer transition-colors';
  return (
    <div className="flex items-center gap-1.5">
      <button type="button" id="neural-graph-zoom-in" title="Zoom in" onClick={onZoomIn} className={btnClass}>
        <ZoomIn className="w-3.5 h-3.5" />
      </button>
      <button type="button" id="neural-graph-zoom-out" title="Zoom out" onClick={onZoomOut} className={btnClass}>
        <ZoomOut className="w-3.5 h-3.5" />
      </button>
      <button type="button" id="neural-graph-reset-view" title="Reset view" onClick={onReset} className={btnClass}>
        <RotateCcw className="w-3.5 h-3.5" />
      </button>
      <button type="button" id="neural-graph-fullscreen" title={isFullscreen ? 'Exit fullscreen' : 'Fullscreen'} onClick={onToggleFullscreen} className={btnClass}>
        {isFullscreen ? <Minimize2 className="w-3.5 h-3.5" /> : <Maximize2 className="w-3.5 h-3.5" />}
      </button>
    </div>
  );
};
