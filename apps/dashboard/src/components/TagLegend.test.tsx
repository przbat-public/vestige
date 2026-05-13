import { render, screen, within } from '@testing-library/react';
import { I18nextProvider } from 'react-i18next';
import i18n from '@/lib/i18n';
import type { GraphNode } from '@/types';
import { TagLegend } from './TagLegend';

function makeNode(id: string, tags: string[]): GraphNode {
  return {
    id,
    label: id,
    type: 'fact',
    retention: 0.7,
    tags,
    createdAt: '',
    updatedAt: '',
    isCenter: false,
  };
}

function renderLegend(nodes: GraphNode[], limit?: number) {
  return render(
    <I18nextProvider i18n={i18n}>
      <TagLegend nodes={nodes} limit={limit} />
    </I18nextProvider>,
  );
}

describe('TagLegend', () => {
  it('renders nothing when there are no tagged nodes', () => {
    const { container } = renderLegend([makeNode('a', []), makeNode('b', [])]);
    expect(container.firstChild).toBeNull();
  });

  it('groups by primary (first) tag and shows the count', () => {
    renderLegend([
      makeNode('a', ['acme', 'web-sdk']),
      makeNode('b', ['acme', 'pacs']),
      makeNode('c', ['acme']),
      makeNode('d', ['personal']),
    ]);
    const list = screen.getByRole('list');
    const acmeItem = within(list).getByText('acme').closest('li');
    expect(acmeItem).toHaveTextContent('3');
    const personalItem = within(list).getByText('personal').closest('li');
    expect(personalItem).toHaveTextContent('1');
  });

  it('sorts by count descending', () => {
    renderLegend([
      makeNode('a', ['rare']),
      makeNode('b', ['common']),
      makeNode('c', ['common']),
      makeNode('d', ['common']),
      makeNode('e', ['mid']),
      makeNode('f', ['mid']),
    ]);
    const list = screen.getByRole('list');
    const items = within(list).getAllByRole('listitem');
    // First three rendered items reflect the sort order: common (3), mid (2), rare (1)
    expect(items[0]).toHaveTextContent('common');
    expect(items[1]).toHaveTextContent('mid');
    expect(items[2]).toHaveTextContent('rare');
  });

  it('honors the limit prop', () => {
    const nodes: GraphNode[] = [];
    for (let i = 0; i < 15; i++) {
      nodes.push(makeNode(`n-${i}`, [`tag-${i}`]));
    }
    renderLegend(nodes, 5);
    expect(screen.getByText('(5)')).toBeInTheDocument();
  });

  it('shows the untagged row when there are tagged AND untagged nodes', () => {
    renderLegend([makeNode('a', ['acme']), makeNode('b', []), makeNode('c', [])]);
    expect(screen.getByText(/untagged/i)).toBeInTheDocument();
    const untaggedRow = screen.getByText(/untagged/i).closest('li');
    expect(untaggedRow).toHaveTextContent('2');
  });

  it('omits the untagged row when every node is tagged', () => {
    renderLegend([makeNode('a', ['acme']), makeNode('b', ['personal'])]);
    expect(screen.queryByText(/untagged/i)).not.toBeInTheDocument();
  });
});
