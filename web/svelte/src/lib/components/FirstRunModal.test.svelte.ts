import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/svelte';
import FirstRunModal from './FirstRunModal.svelte';
import { configApi } from '$lib/api';

// Mock the configApi
vi.mock('$lib/api', () => ({
  configApi: {
    get: vi.fn(),
    update: vi.fn(),
    reset: vi.fn(),
  },
}));

describe('FirstRunModal', () => {
  const mockOnConfirm = vi.fn();
  const mockOnClose = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders the modal with all three input fields', async () => {
    vi.mocked(configApi.get).mockResolvedValue({
      scan_directory: '',
      movies_directory: '',
      tv_directory: '',
    });

    const { getByLabelText, getByText } = render(FirstRunModal, {
      props: { onConfirm: mockOnConfirm, onClose: mockOnClose },
    });

    await waitFor(() => {
      expect(getByLabelText('Scan Directory')).toBeInTheDocument();
      expect(getByLabelText('Movies Directory')).toBeInTheDocument();
      expect(getByLabelText('TV Directory')).toBeInTheDocument();
    });

    expect(getByText('Welcome to Riply')).toBeInTheDocument();
  });

  it('pre-fills fields with existing config values', async () => {
    vi.mocked(configApi.get).mockResolvedValue({
      scan_directory: '/data/scan',
      movies_directory: '/data/movies',
      tv_directory: '/data/tv',
    });

    const { getByLabelText } = render(FirstRunModal, {
      props: { onConfirm: mockOnConfirm, onClose: mockOnClose },
    });

    // Advance timers to allow onMount to execute
    await vi.advanceTimersByTimeAsync(0);

    expect(getByLabelText('Scan Directory').value).toBe('/data/scan');
    expect(getByLabelText('Movies Directory').value).toBe('/data/movies');
    expect(getByLabelText('TV Directory').value).toBe('/data/tv');
  });

  it('shows error when saving with empty fields', async () => {
    vi.mocked(configApi.get).mockResolvedValue({
      scan_directory: '',
      movies_directory: '',
      tv_directory: '',
    });

    const { getByLabelText, getByText, getByRole } = render(FirstRunModal, {
      props: { onConfirm: mockOnConfirm, onClose: mockOnClose },
    });

    await waitFor(() => {
      expect(getByLabelText('Scan Directory')).toBeInTheDocument();
    });

    // Click save without filling fields
    await fireEvent.click(getByRole('button', { name: /Save & Continue/i }));

    await waitFor(() => {
      expect(getByText('All three paths are required.')).toBeInTheDocument();
    });

    // Verify configApi.update was NOT called
    expect(configApi.update).not.toHaveBeenCalled();
  });

  it('calls configApi.update with trimmed values on valid save', async () => {
    vi.mocked(configApi.get).mockResolvedValue({
      scan_directory: '',
      movies_directory: '',
      tv_directory: '',
    });
    vi.mocked(configApi.update).mockResolvedValue({
      scan_directory: '/data/scan',
      movies_directory: '/data/movies',
      tv_directory: '/data/tv',
    });

    const { getByLabelText, getByRole } = render(FirstRunModal, {
      props: { onConfirm: mockOnConfirm, onClose: mockOnClose },
    });

    await waitFor(() => {
      expect(getByLabelText('Scan Directory')).toBeInTheDocument();
    });

    // Fill in fields with extra whitespace
    const scanInput = getByLabelText('Scan Directory');
    const moviesInput = getByLabelText('Movies Directory');
    const tvInput = getByLabelText('TV Directory');

    await fireEvent.input(scanInput, { target: { value: '  /data/scan  ' } });
    await fireEvent.input(moviesInput, { target: { value: '  /data/movies  ' } });
    await fireEvent.input(tvInput, { target: { value: '  /data/tv  ' } });

    // Click save
    await fireEvent.click(getByRole('button', { name: /Save & Continue/i }));

    await waitFor(() => {
      expect(configApi.update).toHaveBeenCalledWith({
        scan_directory: '/data/scan',
        movies_directory: '/data/movies',
        tv_directory: '/data/tv',
      });
    });
  });

  it('calls onConfirm after successful save', async () => {
    vi.mocked(configApi.get).mockResolvedValue({
      scan_directory: '',
      movies_directory: '',
      tv_directory: '',
    });
    vi.mocked(configApi.update).mockResolvedValue({
      scan_directory: '/data/scan',
      movies_directory: '/data/movies',
      tv_directory: '/data/tv',
    });

    const { getByLabelText, getByRole } = render(FirstRunModal, {
      props: { onConfirm: mockOnConfirm, onClose: mockOnClose },
    });

    await waitFor(() => {
      expect(getByLabelText('Scan Directory')).toBeInTheDocument();
    });

    // Fill in fields
    const scanInput = getByLabelText('Scan Directory');
    const moviesInput = getByLabelText('Movies Directory');
    const tvInput = getByLabelText('TV Directory');

    await fireEvent.input(scanInput, { target: { value: '/data/scan' } });
    await fireEvent.input(moviesInput, { target: { value: '/data/movies' } });
    await fireEvent.input(tvInput, { target: { value: '/data/tv' } });

    // Click save
    await fireEvent.click(getByRole('button', { name: /Save & Continue/i }));

    await waitFor(() => {
      expect(mockOnConfirm).toHaveBeenCalledTimes(1);
    });
  });

  it('shows error message when configApi.update fails', async () => {
    vi.mocked(configApi.get).mockResolvedValue({
      scan_directory: '',
      movies_directory: '',
      tv_directory: '',
    });
    vi.mocked(configApi.update).mockRejectedValue(new Error('Network error'));

    const { getByLabelText, getByRole, getByText } = render(FirstRunModal, {
      props: { onConfirm: mockOnConfirm, onClose: mockOnClose },
    });

    await waitFor(() => {
      expect(getByLabelText('Scan Directory')).toBeInTheDocument();
    });

    // Fill in fields
    const scanInput = getByLabelText('Scan Directory');
    const moviesInput = getByLabelText('Movies Directory');
    const tvInput = getByLabelText('TV Directory');

    await fireEvent.input(scanInput, { target: { value: '/data/scan' } });
    await fireEvent.input(moviesInput, { target: { value: '/data/movies' } });
    await fireEvent.input(tvInput, { target: { value: '/data/tv' } });

    // Click save
    await fireEvent.click(getByRole('button', { name: /Save & Continue/i }));

    await waitFor(() => {
      expect(getByText('Failed to save configuration.')).toBeInTheDocument();
    });
  });

  it('calls onClose when clicking Skip button', async () => {
    vi.mocked(configApi.get).mockResolvedValue({
      scan_directory: '',
      movies_directory: '',
      tv_directory: '',
    });

    const { getByRole } = render(FirstRunModal, {
      props: { onConfirm: mockOnConfirm, onClose: mockOnClose },
    });

    await waitFor(() => {
      expect(getByRole('button', { name: 'Skip for now' })).toBeInTheDocument();
    });

    await fireEvent.click(getByRole('button', { name: 'Skip for now' }));

    expect(mockOnClose).toHaveBeenCalledTimes(1);
    expect(mockOnConfirm).not.toHaveBeenCalled();
  });

  it('calls onClose when clicking close button (×)', async () => {
    vi.mocked(configApi.get).mockResolvedValue({
      scan_directory: '',
      movies_directory: '',
      tv_directory: '',
    });

    const { getByRole } = render(FirstRunModal, {
      props: { onConfirm: mockOnConfirm, onClose: mockOnClose },
    });

    await waitFor(() => {
      expect(getByRole('button', { name: '×' })).toBeInTheDocument();
    });

    await fireEvent.click(getByRole('button', { name: '×' }));

    expect(mockOnClose).toHaveBeenCalledTimes(1);
  });

  it('calls onClose when clicking overlay', async () => {
    vi.mocked(configApi.get).mockResolvedValue({
      scan_directory: '',
      movies_directory: '',
      tv_directory: '',
    });

    const { container } = render(FirstRunModal, {
      props: { onConfirm: mockOnConfirm, onClose: mockOnClose },
    });

    await waitFor(() => {
      expect(container.querySelector('.modal-overlay')).toBeInTheDocument();
    });

    // Click the overlay (not the modal content)
    const overlay = container.querySelector('.modal-overlay');
    if (overlay) {
      await fireEvent.click(overlay);
    }

    expect(mockOnClose).toHaveBeenCalledTimes(1);
  });

  it('does not call onClose when clicking modal content', async () => {
    vi.mocked(configApi.get).mockResolvedValue({
      scan_directory: '',
      movies_directory: '',
      tv_directory: '',
    });

    const { container } = render(FirstRunModal, {
      props: { onConfirm: mockOnConfirm, onClose: mockOnClose },
    });

    await waitFor(() => {
      expect(container.querySelector('.first-run-modal')).toBeInTheDocument();
    });

    // Click the modal content (should not close)
    const modal = container.querySelector('.first-run-modal');
    if (modal) {
      await fireEvent.click(modal);
    }

    expect(mockOnClose).not.toHaveBeenCalled();
  });

  it('disables save button while saving', async () => {
    // Simulate a slow API call
    vi.mocked(configApi.get).mockResolvedValue({
      scan_directory: '',
      movies_directory: '',
      tv_directory: '',
    });
    const slowUpdate = vi.fn().mockImplementation(
      () => new Promise((resolve) => setTimeout(() => resolve({}), 100))
    );
    vi.mocked(configApi.update).mockImplementation(slowUpdate);

    const { getByLabelText, getByRole } = render(FirstRunModal, {
      props: { onConfirm: mockOnConfirm, onClose: mockOnClose },
    });

    await waitFor(() => {
      expect(getByLabelText('Scan Directory')).toBeInTheDocument();
    });

    // Fill in fields
    const scanInput = getByLabelText('Scan Directory');
    const moviesInput = getByLabelText('Movies Directory');
    const tvInput = getByLabelText('TV Directory');

    await fireEvent.input(scanInput, { target: { value: '/data/scan' } });
    await fireEvent.input(moviesInput, { target: { value: '/data/movies' } });
    await fireEvent.input(tvInput, { target: { value: '/data/tv' } });

    // Click save
    await fireEvent.click(getByRole('button', { name: /Save & Continue/i }));

    // Button should be disabled while saving
    await waitFor(() => {
      expect(getByRole('button', { name: 'Saving...' })).toBeDisabled();
    });
  });

  it('shows error banner when configApi.get fails', async () => {
    vi.mocked(configApi.get).mockRejectedValue(new Error('Failed to load'));

    const { getByText } = render(FirstRunModal, {
      props: { onConfirm: mockOnConfirm, onClose: mockOnClose },
    });

    await waitFor(() => {
      expect(getByText('Failed to load configuration.')).toBeInTheDocument();
    });
  });
});
