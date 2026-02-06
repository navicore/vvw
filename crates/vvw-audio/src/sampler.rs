/// A looping sampler that plays interleaved stereo audio data in a loop.
pub struct LoopingSampler {
    /// Interleaved stereo samples: [L0, R0, L1, R1, ...]
    data: Box<[f32]>,
    /// Current read position in frames (not samples)
    position: usize,
}

impl LoopingSampler {
    /// Create a new looping sampler from interleaved stereo data.
    ///
    /// `data` must have an even number of elements (pairs of L/R samples).
    pub fn new(data: Vec<f32>) -> Self {
        assert!(data.len().is_multiple_of(2), "data must be interleaved stereo");
        Self {
            data: data.into_boxed_slice(),
            position: 0,
        }
    }

    /// Total number of stereo frames in the buffer
    fn frame_count(&self) -> usize {
        self.data.len() / 2
    }

    /// Generate `num_frames` of audio, writing into the provided left/right buffers.
    /// Wraps around to the beginning when reaching the end of the data.
    pub fn generate(&mut self, left: &mut [f32], right: &mut [f32], num_frames: usize) {
        let total_frames = self.frame_count();
        if total_frames == 0 {
            left[..num_frames].fill(0.0);
            right[..num_frames].fill(0.0);
            return;
        }

        for i in 0..num_frames {
            let idx = self.position * 2;
            left[i] = self.data[idx];
            right[i] = self.data[idx + 1];
            self.position += 1;
            if self.position >= total_frames {
                self.position = 0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_and_loops() {
        // 4 stereo frames: (1,2), (3,4), (5,6), (7,8)
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let mut sampler = LoopingSampler::new(data);

        let mut left = vec![0.0; 6];
        let mut right = vec![0.0; 6];
        sampler.generate(&mut left, &mut right, 6);

        // First 4 frames are the original data, then it wraps
        assert_eq!(left, vec![1.0, 3.0, 5.0, 7.0, 1.0, 3.0]);
        assert_eq!(right, vec![2.0, 4.0, 6.0, 8.0, 2.0, 4.0]);
    }

    #[test]
    fn empty_sampler_fills_zeros() {
        let mut sampler = LoopingSampler::new(vec![]);
        let mut left = vec![1.0; 4];
        let mut right = vec![1.0; 4];
        sampler.generate(&mut left, &mut right, 4);
        assert_eq!(left, vec![0.0; 4]);
        assert_eq!(right, vec![0.0; 4]);
    }
}
