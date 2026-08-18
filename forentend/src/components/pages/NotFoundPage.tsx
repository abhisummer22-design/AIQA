/**
 * FE-4 — real "Not Found" state for any URL that doesn't match a known
 * route (brief §9: "An unknown route must render a proper Not Found state
 * rather than a blank page"). Reuses the existing `EmptyState` primitive
 * (same one FE-3's design language already builds on) rather than
 * introducing a new visual pattern for this.
 */
import React from 'react';
import { useNavigate } from 'react-router-dom';
import { Compass } from 'lucide-react';
import { EmptyState } from '../common/EmptyState';

export const NotFoundPage: React.FC = () => {
  const navigate = useNavigate();
  return (
    <EmptyState
      icon={Compass}
      badge="404"
      title="Page not found"
      description="There is no AIQA workspace at this address. It may have been mistyped, or the link may be out of date."
      actionLabel="Go to Dashboard"
      onAction={() => navigate('/dashboard')}
    />
  );
};
