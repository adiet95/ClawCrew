import { PageHeader, Card } from '@/components/ui';

export default function Instances() {
  return (
    <div className="flex-1 overflow-auto max-w-7xl mx-auto w-full px-6 py-6 pb-20">
      <PageHeader
        title="Instances / Remote Crews"
        description="Manage remote crews and local instance routing."
      />
      <Card className="p-6 text-center text-sm" style={{ color: 'var(--pc-text-faint)' }}>
        <p>Instance registry projection is under construction.</p>
      </Card>
    </div>
  );
}
