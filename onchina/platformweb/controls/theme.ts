import type { ThemeConfig } from 'antd';

export const cidTheme: ThemeConfig = {
  token: {
    colorPrimary: '#0d9488',
    colorSuccess: '#15803d',
    colorWarning: '#d97706',
    colorError: '#dc2626',
    controlHeight: 40,
    borderRadius: 10,
    fontSize: 14,
    colorBgContainer: '#ffffff',
    fontFamily:
      '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Noto Sans SC", "PingFang SC", "Microsoft YaHei", sans-serif'
  },
  components: {
    Button: {
      paddingInline: 24,
      fontWeight: 500
    },
    Card: {
      paddingLG: 32
    },
    Table: {
      headerBg: '#f0fdfa',
      headerColor: '#134e4a',
      headerSplitColor: '#ccfbf1',
      rowHoverBg: '#f0fdfa',
      borderColor: '#e5e7eb'
    },
    Modal: {
      titleFontSize: 18,
      borderRadiusLG: 16
    }
  }
};
