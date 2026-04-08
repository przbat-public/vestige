import { Link } from 'react-router';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';

export function NotFoundPage() {
  const { t } = useTranslation();

  return (
    <div className="flex items-center justify-center h-full">
      <div className="text-center space-y-4">
        <div className="text-4xl text-muted-foreground/50">◌</div>
        <h1 className="text-lg font-semibold text-foreground">{t('notFound.title')}</h1>
        <p className="text-sm text-muted-foreground">{t('notFound.message')}</p>
        <Link to="/graph">
          <Button variant="default" size="md">{t('notFound.goHome')}</Button>
        </Link>
      </div>
    </div>
  );
}
