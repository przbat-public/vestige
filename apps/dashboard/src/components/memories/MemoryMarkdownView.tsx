import ReactMarkdown from 'react-markdown';

interface Props {
  content: string;
}

// Memories often contain markdown structure ([Updated YYYY-MM-DD], bullet
// lists, code blocks, links). Rendering them as markdown keeps long-running
// compound memories readable. Raw HTML stays disabled (no rehype-raw) — this
// is a security-conscious default. The custom components keep heading sizes
// inline with the panel's type scale so wrapped content does not blow out.
export function MemoryMarkdownView({ content }: Props) {
  return (
    <div className="text-sm text-foreground leading-relaxed break-words memory-md prose-sm">
      <ReactMarkdown
        components={{
          h1: ({ children }) => <h3 className="text-base font-semibold mb-1.5 mt-3 first:mt-0">{children}</h3>,
          h2: ({ children }) => <h4 className="text-sm font-semibold mb-1 mt-3 first:mt-0">{children}</h4>,
          h3: ({ children }) => <h5 className="text-xs font-semibold mb-1 mt-2 first:mt-0">{children}</h5>,
          p: ({ children }) => <p className="mb-2 last:mb-0">{children}</p>,
          ul: ({ children }) => <ul className="list-disc list-inside space-y-0.5 mb-2">{children}</ul>,
          ol: ({ children }) => <ol className="list-decimal list-inside space-y-0.5 mb-2">{children}</ol>,
          li: ({ children }) => <li className="text-sm">{children}</li>,
          code: ({ className, children, ...props }) => {
            const isBlock = (className ?? '').includes('language-');
            if (isBlock) {
              return (
                <pre className="bg-accent rounded-md p-2 my-2 overflow-x-auto text-xs">
                  <code className={className} {...props}>
                    {children}
                  </code>
                </pre>
              );
            }
            return <code className="bg-accent rounded px-1 py-0.5 text-xs font-mono">{children}</code>;
          },
          a: ({ children, href }) => (
            <a
              href={href}
              target="_blank"
              rel="noopener noreferrer"
              className="text-primary underline underline-offset-2 hover:no-underline"
            >
              {children}
            </a>
          ),
          blockquote: ({ children }) => (
            <blockquote className="border-l-2 border-border pl-2 italic text-muted-foreground my-2">
              {children}
            </blockquote>
          ),
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  );
}
